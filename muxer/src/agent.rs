use crate::{
    app::App,
    control::{AgentTarget, Request},
    pane::{Pane, Status},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use solmu_client::automation::{self, Action, Metadata, Response, validate_text};
use std::{
    io::{self, Read},
    net::TcpStream,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

#[derive(Debug, Deserialize, Serialize)]
pub struct Failure {
    pub pane: u64,
    pub instance: String,
    pub thread: Option<String>,
    pub failure: Box<automation::Failure>,
}
impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.failure.fmt(f)
    }
}
impl std::error::Error for Failure {}
pub fn resolve<'a>(app: &'a App, target: &AgentTarget) -> Result<&'a Pane, String> {
    match target {
        AgentTarget::Id(id) => app
            .panes
            .iter()
            .find(|pane| pane.id == *id && pane.exited.is_none())
            .ok_or_else(|| "Live Solmu pane does not exist".into()),
        AgentTarget::Name(name) => {
            let mut matches = app
                .panes
                .iter()
                .filter(|pane| pane.exited.is_none() && pane.name.as_deref() == Some(name.trim()));
            let pane = matches.next().ok_or("No live Solmu pane has that name")?;
            if matches.next().is_some() {
                return Err("Agent name is ambiguous; use a pane ID or rename the panes".into());
            }
            Ok(pane)
        }
    }
}
pub fn metadata(pane: &Pane) -> Option<Metadata> {
    if pane.exited.is_some() {
        return None;
    }
    pane.parser
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .callbacks()
        .native
        .clone()
        .filter(|metadata| metadata.instance == pane.instance)
}
pub struct Job {
    pane: u64,
    instance: String,
    thread: Option<String>,
    token: String,
    parser: Arc<Mutex<vt100::Parser<Status>>>,
    action: Action,
    wait: bool,
    deadline: Option<Instant>,
}
impl Job {
    fn failure(
        &self,
        code: &str,
        message: impl Into<String>,
        accepted: Option<bool>,
        turn: Option<String>,
    ) -> Failure {
        let accepted = if accepted == Some(false) && !matches!(self.action, Action::Prompt { .. }) {
            None
        } else {
            accepted
        };
        let turn = turn.or_else(|| match &self.action {
            Action::Wait { turn, .. } => turn.clone(),
            Action::Turn { turn } => Some(turn.clone()),
            _ => None,
        });
        Failure {
            pane: self.pane,
            instance: self.instance.clone(),
            thread: self.thread.clone(),
            failure: Box::new(automation::Failure::new(code, message, accepted, turn)),
        }
    }
    pub fn prepare(app: &App, request: &Request) -> Result<Self, String> {
        let (target, action, wait, timeout) = match request {
            Request::AgentPrompt {
                target,
                text,
                wait,
                timeout_ms,
            } => {
                validate_text(text)?;
                (
                    target,
                    Action::Prompt { text: text.clone() },
                    *wait,
                    *timeout_ms,
                )
            }
            Request::AgentWait {
                target,
                turn,
                timeout_ms,
            } => {
                if turn
                    .as_ref()
                    .is_some_and(|id| uuid::Uuid::parse_str(id).is_err())
                {
                    return Err("Invalid turn ID".into());
                }
                (
                    target,
                    Action::Wait {
                        turn: turn.clone(),
                        timeout_ms: *timeout_ms,
                    },
                    false,
                    *timeout_ms,
                )
            }
            Request::AgentTurn { target, turn } => {
                if uuid::Uuid::parse_str(turn).is_err() {
                    return Err("Invalid turn ID".into());
                }
                (target, Action::Turn { turn: turn.clone() }, false, None)
            }
            Request::AgentStop { target } => (target, Action::Stop, false, None),
            _ => return Err("Not a native agent request".into()),
        };
        if timeout.is_some_and(|ms| !(1..=86400000).contains(&ms)) {
            return Err("Timeout must be 1 through 86400000 milliseconds".into());
        }
        let pane = resolve(app, target)?;
        let token = pane
            .agent_token
            .clone()
            .ok_or("Native Solmu control is unavailable")?;
        let thread = metadata(pane).and_then(|meta| meta.thread).or_else(|| {
            pane.parser
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .callbacks()
                .thread
                .clone()
        });
        Ok(Self {
            pane: pane.id,
            instance: pane.instance.clone(),
            thread,
            token,
            parser: pane.parser.clone(),
            action,
            wait,
            deadline: timeout.map(|ms| Instant::now() + Duration::from_millis(ms)),
        })
    }
    pub fn run(mut self, external: &mut TcpStream) -> Result<Value, Failure> {
        let native = self.await_native(external)?;
        self.thread = native.thread.clone();
        let response = self
            .call(
                &native,
                &self.action,
                external,
                self.deadline,
                !matches!(self.action, Action::Wait { .. }),
            )
            .map_err(|mut error| {
                match &self.action {
                    Action::Wait { turn, .. } => error.failure.turn = turn.clone(),
                    Action::Turn { turn } => error.failure.turn = Some(turn.clone()),
                    _ => {}
                }
                error
            })?;
        if let Response::Accepted { turn } = response {
            if !self.wait {
                return Ok(self.result(json!({"accepted":true,"turn":turn})));
            }
            let timeout_ms = self.deadline.map(|at| {
                at.saturating_duration_since(Instant::now())
                    .as_millis()
                    .max(1) as u64
            });
            let action = Action::Wait {
                turn: Some(turn.id.clone()),
                timeout_ms,
            };
            let response = self
                .call(&native, &action, external, self.deadline, false)
                .map_err(|mut error| {
                    error.failure.accepted = Some(true);
                    error.failure.turn = Some(turn.id.clone());
                    error
                })?;
            return self.finished(response, Some(turn.id));
        }
        self.finished(response, None)
    }
    fn result(&self, mut result: Value) -> Value {
        result["pane"] = json!(self.pane);
        result["instance"] = json!(self.instance);
        result["thread"] = json!(self.thread);
        result
    }
    fn finished(&self, response: Response, accepted: Option<String>) -> Result<Value, Failure> {
        match response {
            Response::Completed { turn } => {
                if matches!(self.action, Action::Turn { .. }) || turn.state == "succeeded" {
                    return Ok(self.result(json!({"accepted":true,"turn":turn})));
                }
                Err(self.failure(
                    &turn.state,
                    turn.error
                        .unwrap_or_else(|| format!("Turn {} was {}", turn.id, turn.state)),
                    Some(true),
                    Some(turn.id),
                ))
            }
            Response::Idle => Ok(self.result(json!({"idle":true}))),
            Response::Stopping => Ok(self.result(json!({"stopping":true}))),
            Response::Error { mut failure } => {
                if let Some(turn) = accepted {
                    failure.accepted = Some(true);
                    failure.turn = Some(turn);
                }
                Err(Failure {
                    pane: self.pane,
                    instance: self.instance.clone(),
                    thread: self.thread.clone(),
                    failure: Box::new(failure),
                })
            }
            Response::Accepted { .. } => Err(self.failure(
                "protocol",
                "Unexpected native Solmu acknowledgement",
                None,
                accepted,
            )),
        }
    }
    fn await_native(&self, external: &TcpStream) -> Result<Metadata, Failure> {
        let startup = Instant::now() + Duration::from_secs(5);
        loop {
            if peer_closed(external) {
                return Err(self.failure("disconnected", "Agent caller disconnected", None, None));
            }
            let parser = self.parser.lock().unwrap_or_else(|e| e.into_inner());
            if parser.callbacks().closed {
                return Err(self.failure(
                    "pane_stopped",
                    "Solmu pane closed before submission",
                    Some(false),
                    None,
                ));
            }
            let native = parser.callbacks().native.clone();
            drop(parser);
            if let Some(meta) =
                native.filter(|meta| meta.instance == self.instance && meta.thread.is_some())
            {
                if self
                    .thread
                    .as_ref()
                    .is_some_and(|id| Some(id) != meta.thread.as_ref())
                {
                    return Err(self.failure(
                        "thread_changed",
                        "Conversation changed before submission",
                        Some(false),
                        None,
                    ));
                }
                return Ok(meta);
            }
            if self.deadline.is_some_and(|at| at <= Instant::now()) || startup <= Instant::now() {
                return Err(self.failure("native_unavailable", "Solmu's native controls did not initialize; use matching current CLI and Muxer binaries", Some(false), None));
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    fn call(
        &self,
        native: &Metadata,
        action: &Action,
        external: &TcpStream,
        deadline: Option<Instant>,
        short: bool,
    ) -> Result<Response, Failure> {
        let request = automation::Request {
            token: self.token.clone(),
            instance: self.instance.clone(),
            thread: self.thread.clone().unwrap(),
            action: match action {
                Action::Prompt { text } => Action::Prompt { text: text.clone() },
                Action::Wait { turn, timeout_ms } => Action::Wait {
                    turn: turn.clone(),
                    timeout_ms: *timeout_ms,
                },
                Action::Turn { turn } => Action::Turn { turn: turn.clone() },
                Action::Stop => Action::Stop,
            },
        };
        let mut stream = TcpStream::connect_timeout(
            &format!("127.0.0.1:{}", native.port).parse().unwrap(),
            Duration::from_secs(1),
        )
        .map_err(|e| {
            self.failure(
                "connection_lost",
                format!("Cannot reach native Solmu: {e}"),
                Some(false),
                None,
            )
        })?;
        stream
            .set_nodelay(true)
            .map_err(|e| self.failure("connection_lost", e.to_string(), Some(false), None))?;
        stream
            .set_write_timeout(Some(Duration::from_secs(2)))
            .map_err(|e| self.failure("connection_lost", e.to_string(), Some(false), None))?;
        stream
            .set_read_timeout(Some(Duration::from_millis(50)))
            .map_err(|e| self.failure("connection_lost", e.to_string(), Some(false), None))?;
        automation::write(&mut stream, &request).map_err(|e| {
            self.failure(
                "connection_lost",
                format!("Native submission failed; inspect before retrying: {e}"),
                None,
                None,
            )
        })?;
        let deadline = if short {
            Some(
                deadline.map_or(Instant::now() + Duration::from_secs(5), |at| {
                    at.min(Instant::now() + Duration::from_secs(5))
                }),
            )
        } else {
            deadline
        };
        let mut bytes = Vec::new();
        let mut chunk = [0; 8192];
        loop {
            if bytes.len() >= 4 {
                let length = u32::from_be_bytes(bytes[..4].try_into().unwrap()) as usize;
                if length > automation::MAX_FRAME {
                    return Err(self.failure(
                        "protocol",
                        "Native reply exceeded its frame limit",
                        None,
                        None,
                    ));
                }
                if bytes.len() >= length + 4 {
                    return serde_json::from_slice(&bytes[4..length + 4]).map_err(|e| {
                        self.failure("protocol", format!("Invalid native reply: {e}"), None, None)
                    });
                }
            }
            if deadline.is_some_and(|at| at <= Instant::now()) {
                return Err(self.failure(
                    "timeout",
                    "Native request timed out; inspect its turn before retrying",
                    None,
                    None,
                ));
            }
            if peer_closed(external) {
                return Err(self.failure("disconnected", "Agent caller disconnected", None, None));
            }
            match stream.read(&mut chunk) {
                Ok(0) => {
                    return Err(self.failure(
                        "connection_lost",
                        "Native Solmu closed; its turn did not complete here",
                        None,
                        None,
                    ));
                }
                Ok(n) => bytes.extend_from_slice(&chunk[..n]),
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                    ) => {}
                Err(error) => {
                    return Err(self.failure(
                        "connection_lost",
                        format!("Native connection failed; inspect before retrying: {error}"),
                        None,
                        None,
                    ));
                }
            }
        }
    }
}
fn peer_closed(stream: &TcpStream) -> bool {
    let _ = stream.set_nonblocking(true);
    let mut byte = [0];
    let result =
        !matches!(stream.peek(&mut byte), Err(error) if error.kind() == io::ErrorKind::WouldBlock);
    let _ = stream.set_nonblocking(false);
    result
}
