use solmu_client::{
    Session, Update,
    automation::{
        Action, Failure, Metadata, Request, Response, Turn, TurnSummary, read, validate_text, write,
    },
};
use std::{
    collections::VecDeque,
    io,
    net::{Shutdown, TcpListener, TcpStream},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::sync::{mpsc, oneshot};

pub struct Command {
    pub request: Request,
    pub reply: oneshot::Sender<Response>,
}
pub struct Bridge {
    pub port: u16,
    pub instance: String,
    pub receiver: mpsc::Receiver<Command>,
    alive: Arc<AtomicBool>,
}
impl Bridge {
    pub fn start() -> io::Result<Self> {
        let (sender, receiver) = mpsc::channel(32);
        let alive = Arc::new(AtomicBool::new(true));
        let Some(token) = std::env::var("SOLMU_MUXER_AGENT_TOKEN")
            .ok()
            .filter(|_| std::env::var_os("SOLMU_MUXER").is_some())
        else {
            return Ok(Self {
                port: 0,
                instance: String::new(),
                receiver,
                alive,
            });
        };
        let instance = std::env::var("SOLMU_MUXER_INSTANCE")
            .map_err(|_| io::Error::other("Missing Muxer instance"))?;
        if uuid::Uuid::parse_str(&token).is_err() || uuid::Uuid::parse_str(&instance).is_err() {
            return Err(io::Error::other("Invalid native Muxer identity"));
        }
        let listener = TcpListener::bind(("127.0.0.1", 0))?;
        let port = listener.local_addr()?.port();
        listener.set_nonblocking(true)?;
        let running = alive.clone();
        let identity = instance.clone();
        std::thread::spawn(move || {
            let workers = Arc::new(AtomicUsize::new(0));
            while running.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        if workers.fetch_add(1, Ordering::Relaxed) >= 32 {
                            workers.fetch_sub(1, Ordering::Relaxed);
                            continue;
                        }
                        let workers = workers.clone();
                        let sender = sender.clone();
                        let token = token.clone();
                        let identity = identity.clone();
                        let running = running.clone();
                        std::thread::spawn(move || {
                            let _ = stream.set_nonblocking(false);
                            let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
                            let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
                            let _ = stream.set_nodelay(true);
                            if let Ok(request) = read::<Request>(&mut stream) {
                                if request.token != token || request.instance != identity {
                                    let _ = write(
                                        &mut stream,
                                        &Response::Error {
                                            failure: Failure::new(
                                                "authentication",
                                                "Native Solmu authentication failed",
                                                Some(false),
                                                None,
                                            ),
                                        },
                                    );
                                } else {
                                    let (reply, mut receive) = oneshot::channel();
                                    if sender.try_send(Command { request, reply }).is_ok() {
                                        let _ = stream.set_nonblocking(true);
                                        loop {
                                            match receive.try_recv() {
                                                Ok(response) => {
                                                    let _ = stream.set_nonblocking(false);
                                                    let _ = write(&mut stream, &response);
                                                    break;
                                                }
                                                Err(oneshot::error::TryRecvError::Closed) => break,
                                                Err(oneshot::error::TryRecvError::Empty) => {}
                                            }
                                            if !running.load(Ordering::Relaxed)
                                                || disconnected(&stream)
                                            {
                                                break;
                                            }
                                            std::thread::sleep(Duration::from_millis(10));
                                        }
                                    } else {
                                        let _ = write(
                                            &mut stream,
                                            &Response::Error {
                                                failure: Failure::new(
                                                    "busy",
                                                    "Native Solmu command queue is full",
                                                    Some(false),
                                                    None,
                                                ),
                                            },
                                        );
                                    }
                                }
                            }
                            let _ = stream.shutdown(Shutdown::Both);
                            workers.fetch_sub(1, Ordering::Relaxed);
                        });
                    }
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(10))
                    }
                    Err(_) => break,
                }
            }
        });
        Ok(Self {
            port,
            instance,
            receiver,
            alive,
        })
    }
}
impl Drop for Bridge {
    fn drop(&mut self) {
        self.alive.store(false, Ordering::Relaxed);
    }
}
fn disconnected(stream: &TcpStream) -> bool {
    let mut byte = [0];
    !matches!(stream.peek(&mut byte), Err(error) if error.kind() == io::ErrorKind::WouldBlock)
}
struct Waiting {
    thread: String,
    turn: Option<String>,
    deadline: Option<Instant>,
    reply: oneshot::Sender<Response>,
}
#[derive(Default)]
pub struct Runtime {
    thread: Option<String>,
    turns: VecDeque<Turn>,
    queue: VecDeque<(String, String)>,
    active: Option<String>,
    waiting: Vec<Waiting>,
}
impl Runtime {
    pub fn queued(&self) -> usize {
        self.queue.len()
    }
    pub fn active(&self) -> bool {
        self.active.is_some()
    }
    fn turn(&self, id: &str) -> Option<&Turn> {
        self.turns.iter().find(|turn| turn.id == id)
    }
    fn turn_mut(&mut self, id: &str) -> Option<&mut Turn> {
        self.turns.iter_mut().find(|turn| turn.id == id)
    }
    fn latest(&self, thread: &str) -> Option<&Turn> {
        self.turns.iter().rev().find(|turn| turn.thread == thread)
    }
    fn create(&mut self, thread: &str, state: &str) -> Turn {
        if self.turns.len() >= 64
            && let Some(index) = self
                .turns
                .iter()
                .position(|turn| !matches!(turn.state.as_str(), "queued" | "working"))
        {
            self.turns.remove(index);
        }
        let turn = Turn {
            id: uuid::Uuid::new_v4().to_string(),
            thread: thread.into(),
            state: state.into(),
            user_message: None,
            assistant_message: None,
            text: None,
            truncated: false,
            error: None,
        };
        self.turns.push_back(turn.clone());
        turn
    }
    pub fn start_if_needed(&mut self, session: &Session) {
        if self.active.is_none()
            && let Some(thread) = &session.current
        {
            self.active = Some(self.create(&thread.id, "working").id);
        }
    }
    pub fn next(&mut self, session: &Session) -> Option<String> {
        if session.busy || self.active.is_some() {
            return None;
        }
        let (id, text) = self.queue.pop_front()?;
        self.turn_mut(&id).unwrap().state = "working".into();
        self.active = Some(id);
        Some(text)
    }
    pub fn observe(&mut self, session: &Session) {
        let current = session.current.as_ref().map(|thread| thread.id.clone());
        if self.thread != current {
            for turn in &mut self.turns {
                if matches!(turn.state.as_str(), "queued" | "working")
                    && Some(&turn.thread) != current.as_ref()
                {
                    turn.state = "failed".into();
                    turn.error = Some("Conversation changed before this turn completed".into());
                }
            }
            self.queue.clear();
            self.active = None;
            self.thread = current;
        }
    }
    pub fn update(&mut self, update: &Update) {
        let Some(id) = self.active.clone() else {
            return;
        };
        let turn = self.turn_mut(&id).unwrap();
        match update {
            Update::Saved(message) if message.role == "user" => {
                turn.user_message = Some(message.id.clone())
            }
            Update::Saved(message) if message.role == "assistant" => {
                turn.assistant_message = Some(message.id.clone());
                let mut end = message.content.len().min(65536);
                while !message.content.is_char_boundary(end) {
                    end -= 1;
                }
                turn.text = Some(message.content[..end].to_owned());
                turn.truncated = end < message.content.len();
                // The saved assistant message completes this turn. A later
                // conversation-list refresh must not change its outcome.
                turn.state = "succeeded".into();
                self.active = None;
            }
            Update::Finished | Update::Stopped | Update::Failed(_) => {
                turn.state = match update {
                    Update::Finished => "succeeded",
                    Update::Stopped => "stopped",
                    _ => "failed",
                }
                .into();
                if let Update::Failed(error) = update {
                    turn.error = Some(error.chars().take(4096).collect());
                }
                self.active = None;
            }
            _ => {}
        }
    }
    pub fn cancel_queue(&mut self) {
        for (id, _) in std::mem::take(&mut self.queue) {
            let turn = self.turn_mut(&id).unwrap();
            turn.state = "stopped".into();
            turn.error = Some("Cancelled before submission".into());
        }
    }
    pub fn handle(&mut self, command: Command, session: &Session) -> bool {
        let Command { request, reply } = command;
        if reply.is_closed() {
            return false;
        }
        if session.current.as_ref().map(|thread| &thread.id) != Some(&request.thread) {
            let (accepted, turn) = match &request.action {
                Action::Prompt { .. } => (Some(false), None),
                Action::Wait { turn, .. } => (None, turn.clone()),
                Action::Turn { turn } => (None, Some(turn.clone())),
                Action::Stop => (None, None),
            };
            let _ = reply.send(Response::Error {
                failure: Failure::new(
                    "thread_changed",
                    "Solmu's conversation changed or is not ready",
                    accepted,
                    turn,
                ),
            });
            return false;
        }
        match request.action {
            Action::Prompt { text } => {
                if let Err(error) = validate_text(&text) {
                    let _ = reply.send(Response::Error {
                        failure: Failure::new("invalid_prompt", error, Some(false), None),
                    });
                } else if self.queue.len() >= 16 {
                    let _ = reply.send(Response::Error {
                        failure: Failure::new(
                            "queue_full",
                            "Sixteen prompts are already queued",
                            Some(false),
                            None,
                        ),
                    });
                } else {
                    let turn = self.create(&request.thread, "queued");
                    self.queue.push_back((turn.id.clone(), text));
                    let _ = reply.send(Response::Accepted { turn });
                }
            }
            Action::Turn { turn } => {
                let response = match self
                    .turn(&turn)
                    .filter(|turn| turn.thread == request.thread)
                {
                    Some(turn) => Response::Completed { turn: turn.clone() },
                    None => Response::Error {
                        failure: Failure::new(
                            "turn_unknown",
                            "This turn is unknown or its retained result expired",
                            None,
                            Some(turn),
                        ),
                    },
                };
                let _ = reply.send(response);
            }
            Action::Wait { turn, timeout_ms } => {
                if timeout_ms.is_some_and(|ms| !(1..=86400000).contains(&ms)) {
                    let _ = reply.send(Response::Error {
                        failure: Failure::new(
                            "invalid_timeout",
                            "Timeout must be 1 through 86400000 milliseconds",
                            None,
                            turn,
                        ),
                    });
                    return false;
                }
                let turn =
                    turn.or_else(|| self.latest(&request.thread).map(|turn| turn.id.clone()));
                if let Some(id) = &turn
                    && self
                        .turn(id)
                        .is_none_or(|turn| turn.thread != request.thread)
                {
                    let _ = reply.send(Response::Error {
                        failure: Failure::new(
                            "turn_unknown",
                            "This turn is unknown or its retained result expired",
                            None,
                            turn,
                        ),
                    });
                } else if self.waiting.len() >= 16 {
                    let _ = reply.send(Response::Error {
                        failure: Failure::new(
                            "wait_limit",
                            "Sixteen native turn waits are already active",
                            None,
                            turn,
                        ),
                    });
                } else {
                    self.waiting.push(Waiting {
                        thread: request.thread,
                        turn,
                        deadline: timeout_ms.map(|ms| Instant::now() + Duration::from_millis(ms)),
                        reply,
                    });
                }
            }
            Action::Stop => {
                self.cancel_queue();
                let _ = reply.send(Response::Stopping);
                return true;
            }
        }
        false
    }
    pub fn poll(&mut self, session: &Session) {
        let mut pending = Vec::new();
        for waiting in std::mem::take(&mut self.waiting) {
            if waiting.reply.is_closed() {
                continue;
            }
            let response =
                if session.current.as_ref().map(|thread| &thread.id) != Some(&waiting.thread) {
                    Some(Response::Error {
                        failure: Failure::new(
                            "thread_changed",
                            "Conversation changed while waiting",
                            waiting.turn.as_ref().map(|_| true),
                            waiting.turn.clone(),
                        ),
                    })
                } else if let Some(id) = &waiting.turn {
                    match self.turn(id) {
                        Some(turn) if !matches!(turn.state.as_str(), "working" | "queued") => {
                            Some(Response::Completed { turn: turn.clone() })
                        }
                        None => Some(Response::Error {
                            failure: Failure::new(
                                "turn_unknown",
                                "Retained turn result expired",
                                None,
                                Some(id.clone()),
                            ),
                        }),
                        _ => None,
                    }
                } else if !session.busy && self.queue.is_empty() {
                    Some(Response::Idle)
                } else {
                    None
                };
            if let Some(response) = response {
                let _ = waiting.reply.send(response);
            } else if waiting.deadline.is_some_and(|at| at <= Instant::now()) {
                let _ = waiting.reply.send(Response::Error {
                    failure: Failure::new(
                        "timeout",
                        "Turn wait timed out; the accepted prompt may still be running or queued",
                        waiting.turn.as_ref().map(|_| true),
                        waiting.turn,
                    ),
                });
            } else {
                pending.push(waiting);
            }
        }
        self.waiting = pending;
    }
    pub fn metadata(&self, bridge: &Bridge, session: &Session) -> Metadata {
        let thread = session.current.as_ref().map(|thread| thread.id.clone());
        Metadata {
            version: 1,
            instance: bridge.instance.clone(),
            port: bridge.port,
            ready: thread.is_some() && !session.busy && self.queue.is_empty(),
            queued: self.queued(),
            turn: thread
                .as_deref()
                .and_then(|thread| self.latest(thread))
                .map(|turn| TurnSummary {
                    id: turn.id.clone(),
                    state: turn.state.clone(),
                }),
            thread,
        }
    }
}
