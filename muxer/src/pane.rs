use portable_pty::{CommandBuilder, MasterPty, PtySize, native_pty_system};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::Widget,
};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        mpsc::{self, SyncSender},
    },
};

#[derive(Default)]
pub struct Status {
    pub label: String,
    pub thread: Option<String>,
    pub native: Option<solmu_client::automation::Metadata>,
    pub closed: bool,
    replies: Vec<Vec<u8>>,
}
impl vt100::Callbacks for Status {
    fn unhandled_osc(&mut self, _: &mut vt100::Screen, params: &[&[u8]]) {
        if params.len() == 3
            && params[0] == b"777"
            && params[1] == b"solmu"
            && let Ok(metadata) =
                serde_json::from_slice::<solmu_client::automation::Metadata>(params[2])
            && metadata.version == 1
            && metadata.port != 0
            && metadata.queued <= 16
            && uuid::Uuid::parse_str(&metadata.instance).is_ok()
            && metadata
                .thread
                .as_ref()
                .is_none_or(|id| uuid::Uuid::parse_str(id).is_ok())
            && metadata.turn.as_ref().is_none_or(|turn| {
                uuid::Uuid::parse_str(&turn.id).is_ok()
                    && matches!(
                        turn.state.as_str(),
                        "queued" | "working" | "succeeded" | "stopped" | "failed"
                    )
            })
        {
            self.native = Some(metadata);
        }
    }
    fn set_window_title(&mut self, _: &mut vt100::Screen, title: &[u8]) {
        if let Some(state) = title.strip_prefix(b"Solmu | ") {
            let state = String::from_utf8_lossy(state);
            let (label, thread) = state.split_once(" | ").unwrap_or((&state, "-"));
            self.label = label.to_owned();
            self.thread = uuid::Uuid::parse_str(thread).ok().map(|id| id.to_string());
        }
    }
    fn unhandled_csi(
        &mut self,
        screen: &mut vt100::Screen,
        first: Option<u8>,
        _: Option<u8>,
        params: &[&[u16]],
        code: char,
    ) {
        let value = params
            .first()
            .and_then(|values| values.first())
            .copied()
            .unwrap_or(0);
        let reply = match (first, code, value) {
            (None, 'n', 6) => {
                let (row, col) = screen.cursor_position();
                format!("\x1b[{};{}R", row + 1, col + 1).into_bytes()
            }
            (None, 'c', 0) => b"\x1b[?1;2c".to_vec(),
            (Some(b'?'), 'u', 0) => b"\x1b[?0u".to_vec(),
            _ => return,
        };
        self.replies.push(reply);
    }
}

pub struct Pane {
    pub id: u64,
    pub instance: String,
    pub name: Option<String>,
    pub directory: PathBuf,
    pub parser: Arc<Mutex<vt100::Parser<Status>>>,
    pub exited: Option<String>,
    pub agent_token: Option<String>,
    pub terminal_lease: Option<u64>,
    master: Option<Box<dyn MasterPty + Send>>,
    child: Option<Box<dyn portable_pty::Child + Send + Sync>>,
    writer: Option<SyncSender<Vec<u8>>>,
    redraw_at: Option<std::time::Instant>,
}
impl Pane {
    pub fn start(
        id: u64,
        directory: PathBuf,
        executable: &Path,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        Self::resume(id, directory, executable, None)
    }
    pub fn resume(
        id: u64,
        directory: PathBuf,
        executable: &Path,
        thread: Option<String>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let pair = native_pty_system().openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })?;
        let mut command = CommandBuilder::new(executable);
        let instance = uuid::Uuid::new_v4().to_string();
        let agent_token = uuid::Uuid::new_v4().to_string();
        if let Some(thread) = &thread {
            command.arg("--thread");
            command.arg(thread);
        }
        command.cwd(&directory);
        command.env("TERM", "xterm-256color");
        command.env("SOLMU_MUXER", "1");
        command.env("SOLMU_MUXER_INSTANCE", &instance);
        command.env("SOLMU_MUXER_AGENT_TOKEN", &agent_token);
        let child = pair.slave.spawn_command(command)?;
        drop(pair.slave);
        let mut terminal_writer = pair.master.take_writer()?;
        let (writer, input_queue) = mpsc::sync_channel::<Vec<u8>>(64);
        std::thread::spawn(move || {
            while let Ok(bytes) = input_queue.recv() {
                if terminal_writer
                    .write_all(&bytes)
                    .and_then(|_| terminal_writer.flush())
                    .is_err()
                {
                    break;
                }
            }
        });
        let mut reader = pair.master.try_clone_reader()?;
        let parser = Arc::new(Mutex::new(vt100::Parser::new_with_callbacks(
            24,
            80,
            2000,
            Status {
                label: "starting".into(),
                thread,
                ..Default::default()
            },
        )));
        let capture = parser.clone();
        let input = writer.clone();
        std::thread::spawn(move || {
            let mut bytes = [0; 8192];
            'read: while let Ok(count) = reader.read(&mut bytes) {
                if count == 0 {
                    break;
                }
                let replies = {
                    let mut parser = capture.lock().unwrap_or_else(|error| error.into_inner());
                    parser.process(&bytes[..count]);
                    std::mem::take(&mut parser.callbacks_mut().replies)
                };
                if !replies.is_empty() {
                    for reply in replies {
                        match input.try_send(reply) {
                            Ok(()) | Err(mpsc::TrySendError::Full(_)) => {}
                            Err(mpsc::TrySendError::Disconnected(_)) => break 'read,
                        }
                    }
                }
            }
            capture
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .callbacks_mut()
                .closed = true;
        });
        Ok(Self {
            id,
            instance,
            name: None,
            directory,
            parser,
            exited: None,
            agent_token: Some(agent_token),
            terminal_lease: None,
            master: Some(pair.master),
            child: Some(child),
            writer: Some(writer),
            redraw_at: None,
        })
    }
    pub fn closed(id: u64, directory: PathBuf, thread: Option<String>, reason: String) -> Self {
        let mut parser = vt100::Parser::new_with_callbacks(
            24,
            80,
            2000,
            Status {
                label: reason.clone(),
                thread,
                ..Default::default()
            },
        );
        parser.process(b"This pane is stopped. Use Restart to start a new conversation.");
        Self {
            id,
            instance: uuid::Uuid::new_v4().to_string(),
            name: None,
            directory,
            parser: Arc::new(Mutex::new(parser)),
            exited: Some(reason),
            agent_token: None,
            terminal_lease: None,
            master: None,
            child: None,
            writer: None,
            redraw_at: None,
        }
    }
    pub fn send(&self, bytes: &[u8]) -> std::io::Result<()> {
        if self.terminal_lease.is_some() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "Pane is controlled by a direct terminal attachment",
            ));
        }
        self.send_direct(bytes)
    }
    pub fn send_direct(&self, bytes: &[u8]) -> std::io::Result<()> {
        if self.exited.is_some() {
            return Ok(());
        }
        if bytes.len() > 65548 {
            return Err(std::io::Error::other("Pane input exceeds 64 KiB"));
        }
        self.writer
            .as_ref()
            .expect("running pane has a writer")
            .try_send(bytes.to_vec())
            .map_err(|error| {
                std::io::Error::new(
                    std::io::ErrorKind::WouldBlock,
                    format!("Pane input queue unavailable: {error}"),
                )
            })
    }
    pub fn pid(&self) -> Option<u32> {
        self.child.as_ref().and_then(|child| child.process_id())
    }
    pub fn poll(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if self
            .redraw_at
            .is_some_and(|deadline| deadline <= std::time::Instant::now())
        {
            self.redraw_at = None;
            if self.exited.is_none() {
                self.parser
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .process(b"\x1b[2J\x1b[H");
                if let Err(error) = self.send_direct(&[12]) {
                    if error.kind() == std::io::ErrorKind::WouldBlock {
                        self.redraw_at =
                            Some(std::time::Instant::now() + std::time::Duration::from_millis(75));
                    } else {
                        return Err(error.into());
                    }
                }
            }
        }
        if self.exited.is_none()
            && let Some(status) = self
                .child
                .as_mut()
                .expect("running pane has a process")
                .try_wait()?
        {
            self.exited = Some(format!("exited {}", status.exit_code()));
            self.child
                .as_mut()
                .expect("running pane has a process")
                .wait()?;
        }
        Ok(())
    }
    pub fn resize(&mut self, area: Rect) -> Result<(), Box<dyn std::error::Error>> {
        if self.terminal_lease.is_some() {
            return Ok(());
        }
        self.resize_direct(area)
    }
    pub fn resize_direct(&mut self, area: Rect) -> Result<(), Box<dyn std::error::Error>> {
        let size = (area.height.max(1), area.width.max(1));
        let mut parser = self
            .parser
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if parser.screen().size() != size {
            if self.exited.is_none()
                && let Some(master) = &self.master
            {
                master.resize(PtySize {
                    rows: size.0,
                    cols: size.1,
                    pixel_width: 0,
                    pixel_height: 0,
                })?;
            }
            parser.screen_mut().set_size(size.0, size.1);
            // ConPTY can emit old-dimension diffs while a resize is in flight.
            // Solmu's Ctrl+L redraw settles the screen once resize events arrive.
            if self.child.is_none() {
                // Restored stopped panes have no process to redraw their notice.
                parser.process(
                    b"\x1b[2J\x1b[HThis pane is stopped. Use Restart to start a new conversation.",
                );
            } else if self.exited.is_none() {
                self.redraw_at =
                    Some(std::time::Instant::now() + std::time::Duration::from_millis(75));
            }
        }
        Ok(())
    }
    pub fn state(&self) -> String {
        self.exited.clone().unwrap_or_else(|| {
            self.parser
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .callbacks()
                .label
                .clone()
        })
    }
    pub fn display_state(&self) -> String {
        let state = self.state();
        let queued = crate::agent::metadata(self).map_or(0, |meta| meta.queued);
        if queued == 0 {
            state
        } else {
            format!("{state} · {queued} queued")
        }
    }
}
impl Drop for Pane {
    fn drop(&mut self) {
        if let Some(child) = &mut self.child {
            if self.exited.is_none() {
                let _ = child.kill();
            }
            let _ = child.wait();
        }
    }
}

pub struct TerminalScreen<'a>(pub &'a vt100::Screen, pub Color, pub Color);
impl Widget for TerminalScreen<'_> {
    fn render(self, area: Rect, buffer: &mut Buffer) {
        let (rows, cols) = self.0.size();
        for row in 0..area.height.min(rows) {
            for col in 0..area.width.min(cols) {
                let Some(cell) = self.0.cell(row, col) else {
                    continue;
                };
                if cell.is_wide_continuation() {
                    continue;
                }
                let mut style = Style::new()
                    .fg(color(cell.fgcolor(), self.1))
                    .bg(color(cell.bgcolor(), self.2));
                for (enabled, modifier) in [
                    (cell.bold(), Modifier::BOLD),
                    (cell.italic(), Modifier::ITALIC),
                    (cell.underline(), Modifier::UNDERLINED),
                    (cell.inverse(), Modifier::REVERSED),
                ] {
                    if enabled {
                        style = style.add_modifier(modifier);
                    }
                }
                let text = cell.contents();
                let remaining = usize::from(area.width - col);
                buffer.set_stringn(
                    area.x + col,
                    area.y + row,
                    if text.is_empty() { " " } else { text },
                    remaining,
                    style,
                );
            }
        }
    }
}
fn color(value: vt100::Color, default: Color) -> Color {
    match value {
        vt100::Color::Default => default,
        vt100::Color::Idx(index) => Color::Indexed(index),
        vt100::Color::Rgb(r, g, b) => Color::Rgb(r, g, b),
    }
}
