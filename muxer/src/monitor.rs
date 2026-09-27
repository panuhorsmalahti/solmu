use crate::{
    app::App,
    control::{Kind, Request},
    session::{Response, packet},
};
use regex::{Regex, RegexBuilder};
use serde_json::{Value, json};
use std::{
    io::Read,
    net::{Shutdown, TcpStream},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc::{self, SyncSender},
    },
    time::{Duration, Instant},
};

pub fn pattern(text: &str, regex: bool) -> Result<Option<Regex>, String> {
    if text.is_empty() || text.len() > 4096 {
        return Err("Output patterns must contain 1 through 4096 UTF-8 bytes".into());
    }
    if regex {
        RegexBuilder::new(text)
            .size_limit(2 * 1024 * 1024)
            .dfa_size_limit(2 * 1024 * 1024)
            .build()
            .map(Some)
            .map_err(|e| format!("Invalid or oversized regular expression: {e}"))
    } else {
        Ok(None)
    }
}
enum Condition {
    State {
        pane: u64,
        instance: String,
        until: Vec<String>,
    },
    Output {
        pane: u64,
        instance: String,
        text: String,
        regex: Option<Regex>,
    },
    Events {
        pane: Option<u64>,
        count: Option<u32>,
        previous: Option<Value>,
        sequence: u64,
    },
}
pub struct Prepared {
    condition: Condition,
    deadline: Option<Instant>,
}
struct Job {
    prepared: Prepared,
    output: SyncSender<Response>,
    alive: Arc<AtomicBool>,
}
#[derive(Default)]
pub struct Manager {
    jobs: Vec<Job>,
    workers: Arc<AtomicUsize>,
}
impl Manager {
    pub fn len(&self) -> usize {
        self.jobs.len()
    }
    pub fn has_subscribers(&self) -> bool {
        self.jobs
            .iter()
            .any(|job| matches!(job.prepared.condition, Condition::Events { .. }))
    }
    pub fn prepare(&self, request: &Request, app: &App) -> Result<Prepared, String> {
        if self.jobs.len() >= 16 || self.workers.load(Ordering::Relaxed) >= 32 {
            return Err("Sixteen monitors or 32 monitor writers are already active".into());
        }
        let instance = |id| {
            app.automation_record(Kind::Pane, id)
                .map(|record| record["instance"].as_str().unwrap().to_owned())
                .map_err(|e| e.to_string())
        };
        let (condition, timeout) = match request {
            Request::Wait {
                pane,
                until,
                timeout_ms,
            } => {
                if until.is_empty()
                    || until.len() > 4
                    || until.iter().any(|state| {
                        !matches!(
                            state.as_str(),
                            "idle" | "working" | "error" | "exited" | "shell" | "running"
                        )
                    })
                {
                    return Err(
                        "Wait states must be idle, working, error, exited, shell, or running"
                            .into(),
                    );
                }
                (
                    Condition::State {
                        pane: *pane,
                        instance: instance(*pane)?,
                        until: until.clone(),
                    },
                    *timeout_ms,
                )
            }
            Request::WaitOutput {
                pane,
                pattern: text,
                regex,
                timeout_ms,
            } => {
                let regex = pattern(text, *regex)?;
                (
                    Condition::Output {
                        pane: *pane,
                        instance: instance(*pane)?,
                        text: text.clone(),
                        regex,
                    },
                    *timeout_ms,
                )
            }
            Request::Subscribe {
                pane,
                timeout_ms,
                count,
            } => {
                if count.is_some_and(|n| !(1..=100000).contains(&n)) {
                    return Err("Event count must be 1 through 100000".into());
                }
                if let Some(pane) = pane {
                    instance(*pane)?;
                }
                (
                    Condition::Events {
                        pane: *pane,
                        count: *count,
                        previous: None,
                        sequence: 0,
                    },
                    *timeout_ms,
                )
            }
            _ => return Err("Not a monitor request".into()),
        };
        if timeout.is_some_and(|ms| !(1..=86400000).contains(&ms)) {
            return Err("Timeout must be 1 through 86400000 milliseconds".into());
        }
        Ok(Prepared {
            condition,
            deadline: timeout.map(|ms| Instant::now() + Duration::from_millis(ms)),
        })
    }
    pub fn accept(&mut self, mut stream: TcpStream, prepared: Prepared, pid: u32) {
        let (output, input) = mpsc::sync_channel(16);
        let alive = Arc::new(AtomicBool::new(true));
        let running = alive.clone();
        self.workers.fetch_add(1, Ordering::Relaxed);
        let workers = self.workers.clone();
        std::thread::spawn(move || {
            if let Ok(mut reader) = stream.try_clone() {
                let running = running.clone();
                std::thread::spawn(move || {
                    let _ = reader.set_read_timeout(None);
                    let mut byte = [0];
                    let _ = reader.read(&mut byte);
                    running.store(false, Ordering::Relaxed);
                    let _ = reader.shutdown(Shutdown::Both);
                });
            }
            if packet(&mut stream, &Response::Ready { pid }).is_ok() {
                while let Ok(response) = input.recv() {
                    if !running.load(Ordering::Relaxed) {
                        break;
                    }
                    let ended = matches!(
                        response,
                        Response::Control(_) | Response::Error(_) | Response::End(_)
                    );
                    if packet(&mut stream, &response).is_err() || ended {
                        break;
                    }
                }
            }
            running.store(false, Ordering::Relaxed);
            let _ = stream.shutdown(Shutdown::Both);
            workers.fetch_sub(1, Ordering::Relaxed);
        });
        self.jobs.push(Job {
            prepared,
            output,
            alive,
        });
    }
    pub fn poll(&mut self, app: &App, snapshot: Option<&Value>) {
        self.jobs.retain_mut(|job| {
            if !job.alive.load(Ordering::Relaxed) {
                return false;
            }
            match evaluate(&mut job.prepared.condition, app, snapshot) {
                Ok(Some((response, finished))) => {
                    if job.output.try_send(response).is_err() {
                        job.alive.store(false, Ordering::Relaxed);
                        return false;
                    }
                    if finished {
                        // Event streams have an explicit normal end; wait replies
                        // are terminal responses themselves.
                        if matches!(job.prepared.condition, Condition::Events { .. }) {
                            let _ = job
                                .output
                                .try_send(Response::End(json!({"reason":"count_reached"})));
                        }
                        return false;
                    }
                }
                Ok(None) => {}
                Err(error) => {
                    let _ = job.output.try_send(Response::Error(error));
                    return false;
                }
            }
            if job.prepared.deadline.is_some_and(|at| at <= Instant::now()) {
                let response = if matches!(job.prepared.condition, Condition::Events { .. }) {
                    Response::End(json!({"reason":"timeout"}))
                } else {
                    Response::Error(
                        "Wait timed out; no matching state or output was observed".into(),
                    )
                };
                let _ = job.output.try_send(response);
                return false;
            }
            true
        });
    }
}
fn occupant(app: &App, pane: u64, instance: &str) -> Result<Value, String> {
    let record = app
        .automation_record(Kind::Pane, pane)
        .map_err(|_| "Pane closed while waiting".to_string())?;
    if record["instance"] != instance {
        return Err("Pane occupant changed while waiting".into());
    }
    Ok(record)
}
fn evaluate(
    condition: &mut Condition,
    app: &App,
    snapshot: Option<&Value>,
) -> Result<Option<(Response, bool)>, String> {
    match condition {
        Condition::State {
            pane,
            instance,
            until,
        } => {
            let record = occupant(app, *pane, instance)?;
            let state = if !record["exited"].is_null() {
                "exited"
            } else {
                record["state"].as_str().unwrap_or("unknown")
            };
            Ok(until.iter().any(|s| s == state).then(|| {
                (
                    Response::Control(json!({"pane":record,"matched_state":state})),
                    true,
                )
            }))
        }
        Condition::Output {
            pane,
            instance,
            text,
            regex,
        } => {
            let record = occupant(app, *pane, instance)?;
            let parser = app
                .panes
                .iter()
                .find(|p| p.id == *pane)
                .unwrap()
                .parser
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            let screen = parser.screen();
            let contents = screen
                .rows(0, screen.size().1)
                .collect::<Vec<_>>()
                .join("\n");
            let matched = match regex {
                Some(regex) => regex.find(&contents).map(|m| (m.start(), m.end())),
                None => contents
                    .find(text.as_str())
                    .map(|start| (start, start + text.len())),
            };
            if let Some((start, end)) = matched {
                return Ok(Some((
                    Response::Control(
                        json!({"pane":record,"matched_text":&contents[start..end],"text":contents,"start":start,"end":end}),
                    ),
                    true,
                )));
            }
            if !record["exited"].is_null() {
                return Err("Pane stopped before output matched".into());
            }
            Ok(None)
        }
        Condition::Events {
            pane,
            count,
            previous,
            sequence,
        } => {
            let state = if let Some(id) = pane {
                app.automation_record(Kind::Pane, *id)
                    .map_err(|_| "Subscribed pane closed".to_string())?
            } else {
                snapshot.expect("subscriber snapshot").clone()
            };
            if previous.as_ref() == Some(&state) {
                return Ok(None);
            }
            *sequence += 1;
            let value = json!({"type":if *sequence == 1 { "snapshot" } else { "changed" },"sequence":sequence,"pane":pane,"snapshot":state});
            *previous = Some(state);
            Ok(Some((
                Response::Event(value),
                count.is_some_and(|count| *sequence >= u64::from(count)),
            )))
        }
    }
}
