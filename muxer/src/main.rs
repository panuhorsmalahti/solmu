mod pane;

use crossterm::event::{
    self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind,
};
use pane::{Pane, TerminalScreen};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Style, Stylize},
    text::Line,
    widgets::{Block, Clear, Paragraph, Wrap},
};
use std::{error::Error, io, path::PathBuf, time::Duration};

struct App {
    panes: Vec<Pane>,
    active: usize,
    next_id: u64,
    executable: PathBuf,
    split: bool,
    prefix: bool,
    workspace: Option<String>,
    notice: String,
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut directories = Vec::new();
    let mut args = std::env::args_os().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--help" || arg == "-h" {
            println!(
                "Solmu muxer\n\nUsage: muxer [--cwd PATH]...\n\nRuns real Solmu CLI terminals. Start solmu-backend first.\nCtrl+b then: n new pane; w new workspace; Tab/] next; [ previous;\n1-8 select; s split/focus view; x close pane; r restart exited pane; q quit.\nClick a pane or sidebar entry to focus. Ctrl+b b sends literal Ctrl+b.\nSOLMU_BACKEND_URL selects the backend; SOLMU_CLI_PATH selects solmu-cli.\nForeground sessions only: quitting terminates the launched CLIs."
            );
            return Ok(());
        } else if arg == "--version" {
            println!("Solmu muxer {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        } else if arg == "--cwd" {
            directories.push(PathBuf::from(
                args.next().ok_or("--cwd requires a directory")?,
            ));
        } else {
            return Err("Unknown option; use muxer --help".into());
        }
    }
    if let Err(error) = dotenvy::dotenv()
        && !error.not_found()
    {
        return Err("Could not load .env".into());
    }
    if directories.is_empty() {
        directories.push(std::env::current_dir()?);
    }
    if directories.len() > 8 {
        return Err("Solmu muxer supports up to eight panes".into());
    }
    let executable = if let Some(path) = std::env::var_os("SOLMU_CLI_PATH") {
        PathBuf::from(path).canonicalize()?
    } else {
        let sibling = std::env::current_exe()?
            .with_file_name(format!("solmu-cli{}", std::env::consts::EXE_SUFFIX));
        if sibling.is_file() {
            sibling
        } else {
            PathBuf::from(format!("solmu-cli{}", std::env::consts::EXE_SUFFIX))
        }
    };
    let mut app = App {
        panes: Vec::new(),
        active: 0,
        next_id: 1,
        executable,
        split: false,
        prefix: false,
        workspace: None,
        notice: String::new(),
    };
    for directory in directories {
        app.add(directory)?;
    }
    app.active = 0;
    let mut terminal = ratatui::init();
    let result = (|| -> Result<(), Box<dyn Error>> {
        crossterm::execute!(
            io::stdout(),
            event::EnableMouseCapture,
            event::EnableBracketedPaste
        )?;
        loop {
            for pane in &mut app.panes {
                pane.poll()?;
            }
            let size = terminal.size()?;
            let area = Rect::new(0, 0, size.width, size.height);
            for (index, rect) in app.visible(area) {
                app.panes[index].resize(Block::bordered().inner(rect))?;
            }
            terminal.draw(|frame| app.draw(frame))?;
            if !event::poll(Duration::from_millis(40))? {
                continue;
            }
            match event::read()? {
                Event::Key(key) if key.kind != KeyEventKind::Release => {
                    if app.key(key)? {
                        break;
                    }
                }
                Event::Paste(text) if app.workspace.is_some() => {
                    if let Some(path) = &mut app.workspace {
                        path.push_str(&text.replace(['\r', '\n'], ""));
                    }
                }
                Event::Paste(text) => {
                    app.panes[app.active].send(text.as_bytes())?;
                }
                Event::Mouse(mouse)
                    if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left))
                        && app.workspace.is_none() =>
                {
                    if mouse.column < sidebar_width(area) && mouse.row >= 2 {
                        let index = usize::from((mouse.row - 2) / 3);
                        if index < app.panes.len() {
                            app.active = index;
                        }
                    } else {
                        for (index, rect) in app.visible(area) {
                            if rect.contains((mouse.column, mouse.row).into()) {
                                app.active = index;
                                break;
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(())
    })();
    let _ = crossterm::execute!(
        io::stdout(),
        event::DisableMouseCapture,
        event::DisableBracketedPaste
    );
    ratatui::restore();
    drop(app); // Kill and reap every owned CLI when the foreground muxer exits.
    result
}

impl App {
    fn add(&mut self, directory: PathBuf) -> Result<(), Box<dyn Error>> {
        if self.panes.len() >= 8 {
            return Err("Eight panes are already open; close one first".into());
        }
        let directory = directory.canonicalize()?;
        if !directory.is_dir() {
            return Err("Workspace must be a directory".into());
        }
        self.panes
            .push(Pane::start(self.next_id, directory, &self.executable)?);
        self.next_id += 1;
        self.active = self.panes.len() - 1;
        self.notice.clear();
        Ok(())
    }
    fn visible(&self, area: Rect) -> Vec<(usize, Rect)> {
        let [_, content] =
            Layout::horizontal([Constraint::Length(sidebar_width(area)), Constraint::Min(0)])
                .areas(area);
        let [body, _] =
            Layout::vertical([Constraint::Min(0), Constraint::Length(2)]).areas(content);
        if self.split && self.panes.len() > 1 {
            let [left, right] = Layout::horizontal([Constraint::Percentage(50); 2]).areas(body);
            vec![
                (self.active, left),
                ((self.active + 1) % self.panes.len(), right),
            ]
        } else {
            vec![(self.active, body)]
        }
    }
    fn key(&mut self, key: KeyEvent) -> Result<bool, Box<dyn Error>> {
        if let Some(path) = &mut self.workspace {
            match key.code {
                KeyCode::Esc => {
                    self.workspace = None;
                    self.notice.clear();
                }
                KeyCode::Backspace => {
                    path.pop();
                }
                KeyCode::Enter => {
                    let directory = PathBuf::from(path.clone());
                    match self.add(directory) {
                        Ok(()) => self.workspace = None,
                        Err(error) => self.notice = error.to_string(),
                    }
                }
                KeyCode::Char(ch)
                    if !key.modifiers.contains(KeyModifiers::CONTROL)
                        || key.modifiers.contains(KeyModifiers::ALT) =>
                {
                    path.push(ch)
                }
                _ => {}
            }
            return Ok(false);
        }
        if self.prefix {
            self.prefix = false;
            self.notice.clear();
            match key.code {
                KeyCode::Char('q') => return Ok(true),
                KeyCode::Char('n') => {
                    let directory = self.panes[self.active].directory.clone();
                    if let Err(error) = self.add(directory) {
                        self.notice = error.to_string();
                    }
                }
                KeyCode::Char('w') => self.workspace = Some(String::new()),
                KeyCode::Tab | KeyCode::Char(']') => {
                    self.active = (self.active + 1) % self.panes.len()
                }
                KeyCode::BackTab | KeyCode::Char('[') => {
                    self.active = (self.active + self.panes.len() - 1) % self.panes.len()
                }
                KeyCode::Char('s') => self.split = !self.split,
                KeyCode::Char('x') => {
                    self.panes.remove(self.active);
                    if self.panes.is_empty() {
                        return Ok(true);
                    }
                    self.active = self.active.min(self.panes.len() - 1);
                }
                KeyCode::Char('r') if self.panes[self.active].exited.is_some() => {
                    let old = &self.panes[self.active];
                    match Pane::start(old.id, old.directory.clone(), &self.executable) {
                        Ok(pane) => self.panes[self.active] = pane,
                        Err(error) => self.notice = error.to_string(),
                    }
                }
                KeyCode::Char('b') => self.panes[self.active].send(&[2])?,
                KeyCode::Char(ch @ '1'..='8') => {
                    let index = ch as usize - '1' as usize;
                    if index < self.panes.len() {
                        self.active = index;
                    }
                }
                _ => self.notice = "Unknown muxer command; Ctrl+b shows shortcuts".into(),
            }
        } else if key.code == KeyCode::Char('b') && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.prefix = true;
        } else {
            self.panes[self.active].send(&encode(key))?;
        }
        Ok(false)
    }
    fn draw(&self, frame: &mut Frame) {
        let area = frame.area();
        let [sidebar, content] =
            Layout::horizontal([Constraint::Length(sidebar_width(area)), Constraint::Min(0)])
                .areas(area);
        let mut lines = vec![Line::from(" SOLMU MUXER".cyan().bold()), Line::from("")];
        for (index, pane) in self.panes.iter().enumerate() {
            let selected = index == self.active;
            let label = format!(
                "{} {} Solmu {}",
                if selected { "›" } else { " " },
                index + 1,
                pane.id
            );
            lines.push(Line::styled(
                label,
                Style::new().fg(if selected { Color::Cyan } else { Color::White }),
            ));
            let name = pane
                .directory
                .file_name()
                .unwrap_or(pane.directory.as_os_str())
                .to_string_lossy();
            lines.push(Line::from(format!("  {name}")));
            lines.push(Line::from(format!("  {}", pane.state())).dark_gray());
        }
        frame.render_widget(
            Paragraph::new(lines).block(Block::new().borders(ratatui::widgets::Borders::RIGHT)),
            sidebar,
        );
        for (index, rect) in self.visible(area) {
            let pane = &self.panes[index];
            let focused = index == self.active;
            let block = Block::bordered()
                .title(format!(" Solmu {} · {} ", pane.id, pane.state()))
                .border_style(Style::new().fg(if focused {
                    Color::Cyan
                } else {
                    Color::DarkGray
                }));
            let inner = block.inner(rect);
            frame.render_widget(block, rect);
            let parser = pane
                .parser
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            frame.render_widget(TerminalScreen(parser.screen()), inner);
            if focused && self.workspace.is_none() && pane.exited.is_none() {
                let (row, col) = parser.screen().cursor_position();
                if row < inner.height && col < inner.width {
                    frame.set_cursor_position((inner.x + col, inner.y + row));
                }
            }
        }
        let footer = Rect::new(content.x, area.bottom().saturating_sub(2), content.width, 2);
        let text = if self.prefix {
            "n new · w workspace · Tab/]/[ switch · 1-8 select · s split · x close · r restart · q quit"
        } else if !self.notice.is_empty() {
            &self.notice
        } else {
            "Ctrl+b commands · click to focus · /exit closes the focused CLI"
        };
        frame.render_widget(
            Paragraph::new(text).dark_gray().wrap(Wrap { trim: false }),
            footer,
        );
        if let Some(path) = &self.workspace {
            let modal = Rect::new(
                area.x + area.width / 8,
                area.y + area.height / 3,
                area.width * 3 / 4,
                6.min(area.height),
            );
            frame.render_widget(Clear, modal);
            frame.render_widget(
                Paragraph::new(format!(
                    "Workspace path: {path}\nEnter creates a Solmu pane · Esc cancels\n{}",
                    self.notice
                ))
                .block(Block::bordered().title(" New workspace "))
                .wrap(Wrap { trim: false }),
                modal,
            );
        }
    }
}
fn sidebar_width(area: Rect) -> u16 {
    24.min(area.width / 3)
}

fn encode(key: KeyEvent) -> Vec<u8> {
    let shift = key.modifiers.contains(KeyModifiers::SHIFT);
    let alt = key.modifiers.contains(KeyModifiers::ALT);
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let modifier = 1 + u8::from(shift) + 2 * u8::from(alt) + 4 * u8::from(ctrl);
    let arrow = |code| {
        if modifier == 1 {
            format!("\x1b[{code}")
        } else {
            format!("\x1b[1;{modifier}{code}")
        }
    };
    let text = match key.code {
        KeyCode::Char(ch) if ctrl && !alt && ch.is_ascii() => {
            let value = ch.to_ascii_uppercase() as u8;
            if (b'@'..=b'_').contains(&value) {
                String::from_utf8(vec![value & 0x1f]).unwrap_or_default()
            } else {
                return Vec::new();
            }
        }
        KeyCode::Char(ch) => ch.to_string(),
        KeyCode::Enter => "\r".into(),
        KeyCode::Tab => "\t".into(),
        KeyCode::BackTab => "\x1b[Z".into(),
        KeyCode::Backspace => "\x7f".into(),
        KeyCode::Esc => "\x1b".into(),
        KeyCode::Up => arrow('A'),
        KeyCode::Down => arrow('B'),
        KeyCode::Right => arrow('C'),
        KeyCode::Left => arrow('D'),
        KeyCode::Home => arrow('H'),
        KeyCode::End => arrow('F'),
        KeyCode::PageUp => "\x1b[5~".into(),
        KeyCode::PageDown => "\x1b[6~".into(),
        KeyCode::Delete => "\x1b[3~".into(),
        KeyCode::Insert => "\x1b[2~".into(),
        _ => return Vec::new(),
    };
    if alt
        && !ctrl
        && matches!(
            key.code,
            KeyCode::Char(_) | KeyCode::Enter | KeyCode::Backspace
        )
    {
        format!("\x1b{text}").into_bytes()
    } else {
        text.into_bytes()
    }
}
