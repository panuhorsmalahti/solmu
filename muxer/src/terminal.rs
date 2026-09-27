//! Direct attachment to a running pane, independent of the workspace view.
use crate::{
    app::App,
    control::{AgentTarget, Request},
    session::{self, Response},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyModifiers},
    terminal,
};
use ratatui::layout::Rect;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    error::Error,
    io::{self, BufRead, IsTerminal, Read, Write},
    net::{Shutdown, TcpStream},
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
        mpsc::{self, Receiver, SyncSender},
    },
    time::Duration,
};

#[derive(Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    Input { data: String },
    Resize { cols: u16, rows: u16 },
    Scroll { lines: i32 },
    Release,
}
#[derive(Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Record {
    Frame {
        pane: u64,
        instance: String,
        cols: u16,
        rows: u16,
        bracketed_paste: bool,
        data: String,
    },
    Closed {
        reason: String,
    },
    Error {
        message: String,
    },
}
pub fn validate_size(cols: Option<u16>, rows: Option<u16>) -> Result<(), String> {
    if cols.is_some_and(|v| !(1..=240).contains(&v))
        || rows.is_some_and(|v| !(1..=100).contains(&v))
    {
        return Err("Terminal dimensions must be 1–240 columns and 1–100 rows".into());
    }
    Ok(())
}
fn input(data: &str) -> Result<Vec<u8>, String> {
    if data.len() > 87384 {
        return Err("Terminal input exceeds 64 KiB".into());
    }
    let bytes = STANDARD
        .decode(data)
        .map_err(|_| "Terminal input must be base64".to_owned())?;
    if bytes.len() > 65536 {
        return Err("Terminal input exceeds 64 KiB".into());
    }
    Ok(bytes)
}
struct Peer {
    pane: u64,
    instance: String,
    observe: bool,
    cols: u16,
    rows: u16,
    before: (u16, u16),
    output: SyncSender<Response>,
    previous: Vec<u8>,
}
enum Incoming {
    Command(u64, Command),
    Closed(u64),
}
pub struct Manager {
    peers: BTreeMap<u64, Peer>,
    sender: SyncSender<Incoming>,
    incoming: Receiver<Incoming>,
    next_id: u64,
    workers: Arc<AtomicUsize>,
}
impl Default for Manager {
    fn default() -> Self {
        let (sender, incoming) = mpsc::sync_channel(128);
        Self {
            peers: BTreeMap::new(),
            sender,
            incoming,
            next_id: 1,
            workers: Arc::new(AtomicUsize::new(0)),
        }
    }
}
impl Manager {
    pub fn accept(
        &mut self,
        stream: &mut TcpStream,
        app: &mut App,
        request: &Request,
        pid: u32,
    ) -> Result<(), String> {
        let Request::TerminalOpen {
            target,
            observe,
            takeover,
            cols,
            rows,
        } = request
        else {
            unreachable!()
        };
        validate_size(*cols, *rows)?;
        if *observe && *takeover {
            return Err("Observers cannot take over a terminal".into());
        }
        if self.workers.load(Ordering::Relaxed) >= 32 {
            return Err("Direct terminal connection limit reached".into());
        }
        let pane = match target {
            AgentTarget::Id(id) if *observe => app
                .panes
                .iter()
                .find(|pane| pane.id == *id)
                .ok_or("Pane does not exist")?,
            _ => crate::agent::resolve(app, target)?,
        };
        if self.peers.len() >= 16 && !(!*observe && *takeover && pane.terminal_lease.is_some()) {
            return Err("Direct terminal connection limit reached".into());
        }
        let id = pane.id;
        let current_size = pane
            .parser
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .screen()
            .size();
        let mut before = current_size;
        if !*observe && let Some(owner) = pane.terminal_lease {
            if !*takeover {
                return Err("Terminal already has a controller; use --takeover".into());
            }
            if let Some(peer) = self.peers.get(&owner) {
                before = peer.before;
            }
            self.close(app, owner, "taken_over");
        }
        let cols = cols.unwrap_or(current_size.1);
        let rows = rows.unwrap_or(current_size.0);
        let peer_id = self.next_id;
        let pane = app.panes.iter_mut().find(|pane| pane.id == id).unwrap();
        let instance = pane.instance.clone();
        if !*observe {
            pane.resize_direct(Rect::new(0, 0, cols, rows))
                .map_err(|e| e.to_string())?;
        }
        let mut writer = stream.try_clone().map_err(|e| e.to_string())?;
        let mut reader = stream.try_clone().map_err(|e| e.to_string())?;
        reader.set_read_timeout(None).map_err(|e| e.to_string())?;
        writer
            .set_write_timeout(Some(Duration::from_secs(3)))
            .map_err(|e| e.to_string())?;
        let (output, responses) = mpsc::sync_channel(4);
        output
            .try_send(Response::Ready { pid })
            .map_err(|e| e.to_string())?;
        self.workers.fetch_add(1, Ordering::Relaxed);
        let workers = self.workers.clone();
        std::thread::spawn(move || {
            while let Ok(response) = responses.recv() {
                if session::packet(&mut writer, &response).is_err() {
                    break;
                }
            }
            let _ = writer.shutdown(Shutdown::Both);
            workers.fetch_sub(1, Ordering::Relaxed);
        });
        let sender = self.sender.clone();
        std::thread::spawn(move || {
            while let Ok(session::Request::Terminal(command)) = session::receive(&mut reader) {
                if sender.send(Incoming::Command(peer_id, command)).is_err() {
                    return;
                }
            }
            let _ = sender.send(Incoming::Closed(peer_id));
        });
        if !*observe {
            pane.terminal_lease = Some(peer_id);
        }
        self.peers.insert(
            peer_id,
            Peer {
                pane: id,
                instance,
                observe: *observe,
                cols,
                rows,
                before,
                output,
                previous: Vec::new(),
            },
        );
        self.next_id += 1;
        Ok(())
    }
    fn close(&mut self, app: &mut App, id: u64, reason: &str) {
        if let Some(peer) = self.peers.remove(&id) {
            let _ = peer.output.try_send(Response::Terminal(Record::Closed {
                reason: reason.into(),
            }));
            if let Some(pane) = app.panes.iter_mut().find(|p| {
                p.id == peer.pane && p.instance == peer.instance && p.terminal_lease == Some(id)
            }) {
                pane.terminal_lease = None;
                // Restore the pre-attachment size until a workspace viewer resizes it.
                let _ = pane.resize_direct(Rect::new(0, 0, peer.before.1, peer.before.0));
            }
        }
    }
    fn command(&mut self, app: &mut App, id: u64, command: Command) -> Result<(), String> {
        if matches!(command, Command::Release) {
            self.close(app, id, "released");
            return Ok(());
        }
        let peer = self
            .peers
            .get_mut(&id)
            .ok_or("Terminal attachment has ended")?;
        if peer.observe {
            return Err("Observer connections cannot send input, resize, or scroll".into());
        }
        let pane = app
            .panes
            .iter_mut()
            .find(|p| {
                p.id == peer.pane && p.instance == peer.instance && p.terminal_lease == Some(id)
            })
            .ok_or("Terminal process was replaced or closed")?;
        if pane.exited.is_some() {
            return Err("Terminal process has exited".into());
        }
        match command {
            Command::Input { data } => pane
                .send_direct(&input(&data)?)
                .map_err(|e| e.to_string())?,
            Command::Resize { cols, rows } => {
                validate_size(Some(cols), Some(rows))?;
                pane.resize_direct(Rect::new(0, 0, cols, rows))
                    .map_err(|e| e.to_string())?;
                peer.cols = cols;
                peer.rows = rows;
            }
            Command::Scroll { lines } => {
                if lines.unsigned_abs() > 2000 {
                    return Err("Scroll is limited to 2000 rows per command".into());
                }
                let mut parser = pane.parser.lock().unwrap_or_else(|e| e.into_inner());
                let offset = parser
                    .screen()
                    .scrollback()
                    .saturating_add_signed(lines as isize);
                parser.screen_mut().set_scrollback(offset);
            }
            Command::Release => unreachable!(),
        }
        Ok(())
    }
    pub fn poll(&mut self, app: &mut App) {
        for _ in 0..128 {
            let Ok(message) = self.incoming.try_recv() else {
                break;
            };
            match message {
                Incoming::Closed(id) => self.close(app, id, "disconnected"),
                Incoming::Command(id, command) => {
                    if !self.peers.contains_key(&id) {
                        continue;
                    }
                    if let Err(message) = self.command(app, id, command)
                        && let Some(peer) = self.peers.get(&id)
                    {
                        let _ = peer
                            .output
                            .try_send(Response::Terminal(Record::Error { message }));
                    }
                }
            }
        }
        let mut closed = Vec::new();
        for (id, peer) in &mut self.peers {
            let Some(pane) = app.panes.iter().find(|pane| pane.id == peer.pane) else {
                closed.push((*id, "pane_closed"));
                continue;
            };
            if pane.instance != peer.instance {
                closed.push((*id, "restarted"));
                continue;
            }
            let parser = pane.parser.lock().unwrap_or_else(|e| e.into_inner());
            let mut screen = parser.screen().clone();
            // An observer's viewport is a projection: it never resizes the PTY.
            screen.set_size(peer.rows, peer.cols);
            let bytes = screen.state_formatted();
            if bytes != peer.previous {
                let frame = Record::Frame {
                    pane: peer.pane,
                    instance: peer.instance.clone(),
                    cols: peer.cols,
                    rows: peer.rows,
                    bracketed_paste: screen.bracketed_paste(),
                    data: STANDARD.encode(&bytes),
                };
                if peer.output.try_send(Response::Terminal(frame)).is_err() {
                    closed.push((*id, "slow_reader"));
                    continue;
                }
                peer.previous = bytes;
            }
            if pane.exited.is_some() || parser.callbacks().closed {
                closed.push((*id, "exited"));
            }
        }
        for (id, reason) in closed {
            self.close(app, id, reason);
        }
    }
}
impl Drop for Manager {
    fn drop(&mut self) {
        for peer in self.peers.values() {
            let _ = peer.output.try_send(Response::Terminal(Record::Closed {
                reason: "server_stopped".into(),
            }));
        }
    }
}

struct TerminalGuard {
    was_raw: bool,
}
impl TerminalGuard {
    fn new() -> io::Result<Self> {
        let guard = Self {
            was_raw: terminal::is_raw_mode_enabled()?,
        };
        terminal::enable_raw_mode()?;
        crossterm::execute!(
            io::stdout(),
            terminal::EnterAlternateScreen,
            event::EnableBracketedPaste
        )?;
        Ok(guard)
    }
}
impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = crossterm::execute!(
            io::stdout(),
            cursor::Show,
            event::DisableMouseCapture,
            event::DisableBracketedPaste,
            terminal::LeaveAlternateScreen
        );
        if !self.was_raw {
            let _ = terminal::disable_raw_mode();
        }
    }
}
pub fn run(name: &str, mut request: Request, interactive: bool) -> Result<(), Box<dyn Error>> {
    let Request::TerminalOpen {
        observe,
        cols,
        rows,
        ..
    } = &mut request
    else {
        unreachable!()
    };
    let observe = *observe;
    if interactive {
        if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
            return Err(
                "Direct attach needs a terminal; use terminal session control for scripts".into(),
            );
        }
        let size = terminal::size()?;
        *cols = Some(
            cols.unwrap_or(
                size.0
                    .saturating_sub(u16::from(cfg!(windows)))
                    .clamp(1, 240),
            ),
        );
        *rows = Some(rows.unwrap_or(size.1.clamp(1, 100)));
    }
    let mut stream =
        session::connect_request(&session::root()?, name, "control", (80, 24), Some(request))?;
    let _guard = interactive.then(TerminalGuard::new).transpose()?;
    let input_error = Arc::new(Mutex::new(None::<String>));
    let running = Arc::new(std::sync::atomic::AtomicBool::new(true));
    let paste_mode = Arc::new(std::sync::atomic::AtomicBool::new(false));
    if interactive || !observe {
        let mut sender = stream.try_clone()?;
        let error = input_error.clone();
        let active = running.clone();
        let paste = paste_mode.clone();
        std::thread::spawn(move || {
            let result = if interactive {
                interactive_input(&mut sender, &active, &paste)
            } else {
                json_input(&mut sender)
            };
            if let Err(e) = result {
                *error.lock().unwrap_or_else(|e| e.into_inner()) = Some(e.to_string());
                let _ = sender.shutdown(Shutdown::Both);
            }
        });
    }
    let result = (|| -> Result<(), Box<dyn Error>> {
        loop {
            let record = match session::receive(&mut stream)? {
                Response::Terminal(record) => record,
                _ => return Err("Unexpected terminal stream response".into()),
            };
            if interactive {
                match record {
                    Record::Frame {
                        data,
                        bracketed_paste,
                        ..
                    } => {
                        let bytes = STANDARD.decode(data)?;
                        paste_mode.store(bracketed_paste, Ordering::Relaxed);
                        let mut out = io::stdout().lock();
                        out.write_all(&bytes)?;
                        out.flush()?;
                    }
                    Record::Closed { .. } => return Ok(()),
                    Record::Error { message } => return Err(message.into()),
                }
            } else {
                let closed = matches!(record, Record::Closed { .. });
                let mut out = io::stdout().lock();
                writeln!(out, "{}", serde_json::to_string(&record)?)?;
                out.flush()?;
                if closed {
                    return Ok(());
                }
            }
        }
    })();
    running.store(false, Ordering::Relaxed);
    let _ = stream.shutdown(Shutdown::Both);
    if let Some(error) = input_error.lock().unwrap_or_else(|e| e.into_inner()).take() {
        return Err(error.into());
    }
    result
}
fn json_input(stream: &mut TcpStream) -> Result<(), Box<dyn Error>> {
    let mut reader = io::stdin().lock();
    loop {
        let mut line = Vec::new();
        let count = reader.by_ref().take(131073).read_until(b'\n', &mut line)?;
        if count == 0 {
            session::packet(stream, &session::Request::Terminal(Command::Release))?;
            return Ok(());
        }
        if line.len() > 131072 {
            return Err("Terminal command exceeds 128 KiB".into());
        }
        let command: Command = serde_json::from_slice(&line)?;
        let released = matches!(command, Command::Release);
        session::packet(stream, &session::Request::Terminal(command))?;
        if released {
            return Ok(());
        }
    }
}
fn interactive_input(
    stream: &mut TcpStream,
    running: &std::sync::atomic::AtomicBool,
    paste: &std::sync::atomic::AtomicBool,
) -> Result<(), Box<dyn Error>> {
    let mut prefix = false;
    while running.load(Ordering::Relaxed) {
        if !event::poll(Duration::from_millis(100))? {
            continue;
        }
        let command = match event::read()? {
            Event::Key(key) if key.kind != event::KeyEventKind::Release => {
                let ctrl_b =
                    key.code == KeyCode::Char('b') && key.modifiers == KeyModifiers::CONTROL;
                if prefix && key.code == KeyCode::Char('q') && key.modifiers.is_empty() {
                    session::packet(stream, &session::Request::Terminal(Command::Release))?;
                    return Ok(());
                }
                if !prefix && ctrl_b {
                    prefix = true;
                    continue;
                }
                let mut bytes = if prefix && !ctrl_b {
                    vec![2]
                } else {
                    Vec::new()
                };
                bytes.extend(crate::keys::encode(key));
                prefix = false;
                Command::Input {
                    data: STANDARD.encode(bytes),
                }
            }
            Event::Paste(text) => Command::Input {
                data: STANDARD.encode(if paste.load(Ordering::Relaxed) {
                    format!("\x1b[200~{text}\x1b[201~")
                } else {
                    text
                }),
            },
            Event::Resize(cols, rows) => Command::Resize {
                cols: cols.saturating_sub(u16::from(cfg!(windows))).clamp(1, 240),
                rows: rows.clamp(1, 100),
            },
            _ => continue,
        };
        session::packet(stream, &session::Request::Terminal(command))?;
    }
    Ok(())
}
