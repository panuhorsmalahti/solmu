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
    sync::{Arc, Mutex},
};

#[derive(Default)]
pub struct Status {
    pub label: String,
    replies: Vec<Vec<u8>>,
}
impl vt100::Callbacks for Status {
    fn set_window_title(&mut self, _: &mut vt100::Screen, title: &[u8]) {
        if let Some(state) = title.strip_prefix(b"Solmu | ") {
            self.label = String::from_utf8_lossy(state).into_owned();
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
    pub directory: PathBuf,
    pub parser: Arc<Mutex<vt100::Parser<Status>>>,
    pub exited: Option<String>,
    master: Box<dyn MasterPty + Send>,
    child: Box<dyn portable_pty::Child + Send + Sync>,
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
}
impl Pane {
    pub fn start(
        id: u64,
        directory: PathBuf,
        executable: &Path,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let pair = native_pty_system().openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })?;
        let mut command = CommandBuilder::new(executable);
        command.cwd(&directory);
        command.env("TERM", "xterm-256color");
        let child = pair.slave.spawn_command(command)?;
        drop(pair.slave);
        let writer = Arc::new(Mutex::new(pair.master.take_writer()?));
        let mut reader = pair.master.try_clone_reader()?;
        let parser = Arc::new(Mutex::new(vt100::Parser::new_with_callbacks(
            24,
            80,
            2000,
            Status {
                label: "starting".into(),
                ..Default::default()
            },
        )));
        let capture = parser.clone();
        let input = writer.clone();
        std::thread::spawn(move || {
            let mut bytes = [0; 8192];
            while let Ok(count) = reader.read(&mut bytes) {
                if count == 0 {
                    break;
                }
                let replies = {
                    let mut parser = capture.lock().unwrap_or_else(|error| error.into_inner());
                    parser.process(&bytes[..count]);
                    std::mem::take(&mut parser.callbacks_mut().replies)
                };
                if !replies.is_empty() {
                    let mut writer = input.lock().unwrap_or_else(|error| error.into_inner());
                    for reply in replies {
                        if writer.write_all(&reply).is_err() {
                            return;
                        }
                    }
                    let _ = writer.flush();
                }
            }
        });
        Ok(Self {
            id,
            directory,
            parser,
            exited: None,
            master: pair.master,
            child,
            writer,
        })
    }
    pub fn send(&self, bytes: &[u8]) -> std::io::Result<()> {
        if self.exited.is_some() {
            return Ok(());
        }
        let mut writer = self
            .writer
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        writer.write_all(bytes)?;
        writer.flush()
    }
    pub fn poll(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if self.exited.is_none()
            && let Some(status) = self.child.try_wait()?
        {
            self.exited = Some(format!("exited {}", status.exit_code()));
            self.child.wait()?;
        }
        Ok(())
    }
    pub fn resize(&mut self, area: Rect) -> Result<(), Box<dyn std::error::Error>> {
        let size = (area.height.max(1), area.width.max(1));
        let mut parser = self
            .parser
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if parser.screen().size() != size {
            self.master.resize(PtySize {
                rows: size.0,
                cols: size.1,
                pixel_width: 0,
                pixel_height: 0,
            })?;
            parser.screen_mut().set_size(size.0, size.1);
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
}
impl Drop for Pane {
    fn drop(&mut self) {
        if self.exited.is_none() {
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
    }
}

pub struct TerminalScreen<'a>(pub &'a vt100::Screen);
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
                    .fg(color(cell.fgcolor()))
                    .bg(color(cell.bgcolor()));
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
fn color(value: vt100::Color) -> Color {
    match value {
        vt100::Color::Default => Color::Reset,
        vt100::Color::Idx(index) => Color::Indexed(index),
        vt100::Color::Rgb(r, g, b) => Color::Rgb(r, g, b),
    }
}
