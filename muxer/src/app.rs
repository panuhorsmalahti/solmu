use crate::{
    keys::encode,
    pane::{Pane, TerminalScreen},
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Style, Stylize},
    text::Line,
    widgets::{Block, Clear, Paragraph, Wrap},
};
use std::{error::Error, path::PathBuf};

const BG: Color = Color::Rgb(21, 26, 35);
const CHROME: Color = Color::Rgb(30, 37, 48);
const SELECTED: Color = Color::Rgb(45, 60, 65);
const TEXT: Color = Color::Rgb(224, 222, 244);
const MUTED: Color = Color::Rgb(136, 149, 166);
const ACCENT: Color = Color::Rgb(156, 207, 176);

struct Space {
    directory: PathBuf,
    tabs: Vec<u64>,
    selected: u64,
}
pub struct App {
    pub panes: Vec<Pane>,
    pub active: usize,
    pub workspace: Option<String>,
    spaces: Vec<Space>,
    space: usize,
    next_id: u64,
    executable: PathBuf,
    split: bool,
    prefix: bool,
    notice: String,
}
impl App {
    pub fn new(executable: PathBuf) -> Self {
        Self {
            panes: vec![],
            spaces: vec![],
            active: 0,
            workspace: None,
            space: 0,
            next_id: 1,
            executable,
            split: false,
            prefix: false,
            notice: String::new(),
        }
    }
    pub fn add_space(&mut self, directory: PathBuf) -> Result<(), Box<dyn Error>> {
        if self.spaces.len() >= 8 {
            return Err("Eight spaces are already open; close a space's tabs first".into());
        }
        let directory = directory.canonicalize()?;
        if !directory.is_dir() {
            return Err("Workspace must be a directory".into());
        }
        let pane = Pane::start(self.next_id, directory.clone(), &self.executable)?;
        self.spaces.push(Space {
            directory,
            tabs: vec![pane.id],
            selected: pane.id,
        });
        self.panes.push(pane);
        self.space = self.spaces.len() - 1;
        self.active = self.panes.len() - 1;
        self.next_id += 1;
        self.notice.clear();
        Ok(())
    }
    fn add_tab(&mut self) -> Result<(), Box<dyn Error>> {
        if self.spaces[self.space].tabs.len() >= 8 {
            return Err("Eight tabs are already open in this space".into());
        }
        let pane = Pane::start(
            self.next_id,
            self.spaces[self.space].directory.clone(),
            &self.executable,
        )?;
        self.spaces[self.space].tabs.push(pane.id);
        self.spaces[self.space].selected = pane.id;
        self.panes.push(pane);
        self.active = self.panes.len() - 1;
        self.next_id += 1;
        self.notice.clear();
        Ok(())
    }
    pub fn select_space(&mut self, index: usize) {
        self.space = index;
        self.active = self
            .panes
            .iter()
            .position(|pane| pane.id == self.spaces[index].selected)
            .expect("selected tab exists");
    }
    fn focus(&mut self, index: usize) {
        let id = self.panes[index].id;
        self.active = index;
        self.space = self
            .spaces
            .iter()
            .position(|space| space.tabs.contains(&id))
            .expect("tab owns a space");
        self.spaces[self.space].selected = id;
    }
    fn select_tab(&mut self, position: usize) {
        if let Some(id) = self.spaces[self.space].tabs.get(position) {
            self.focus(self.panes.iter().position(|pane| pane.id == *id).unwrap());
        }
    }
    fn close(&mut self, id: u64) -> bool {
        let space = self
            .spaces
            .iter()
            .position(|space| space.tabs.contains(&id))
            .unwrap();
        let position = self.spaces[space]
            .tabs
            .iter()
            .position(|tab| *tab == id)
            .unwrap();
        self.spaces[space].tabs.remove(position);
        self.panes
            .remove(self.panes.iter().position(|pane| pane.id == id).unwrap());
        if self.panes.is_empty() {
            return true;
        }
        if self.spaces[space].tabs.is_empty() {
            self.spaces.remove(space);
            self.space = self.space.min(self.spaces.len() - 1);
        } else if self.spaces[space].selected == id {
            let tabs = &self.spaces[space].tabs;
            self.spaces[space].selected = tabs[position.min(tabs.len() - 1)];
        }
        self.select_space(self.space);
        false
    }
    fn submit(&mut self) {
        if let Some(path) = self.workspace.clone() {
            match self.add_space(path.into()) {
                Ok(()) => self.workspace = None,
                Err(error) => self.notice = error.to_string(),
            }
        }
    }
    fn restart(&mut self) {
        let old = &self.panes[self.active];
        if old.exited.is_some() {
            match Pane::start(old.id, old.directory.clone(), &self.executable) {
                Ok(pane) => self.panes[self.active] = pane,
                Err(error) => self.notice = error.to_string(),
            }
        }
    }
    pub fn key(&mut self, key: KeyEvent) -> Result<bool, Box<dyn Error>> {
        if let Some(path) = &mut self.workspace {
            match key.code {
                KeyCode::Esc => {
                    self.workspace = None;
                    self.notice.clear();
                }
                KeyCode::Backspace => {
                    path.pop();
                }
                KeyCode::Enter => self.submit(),
                KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => path.clear(),
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
            let tabs = &self.spaces[self.space].tabs;
            let position = tabs
                .iter()
                .position(|id| *id == self.panes[self.active].id)
                .unwrap();
            match key.code {
                KeyCode::Char('q') => return Ok(true),
                KeyCode::Char('n') => {
                    if let Err(error) = self.add_tab() {
                        self.notice = error.to_string();
                    }
                }
                KeyCode::Char('w') => self.workspace = Some(String::new()),
                KeyCode::Tab | KeyCode::Char(']') => self.select_tab((position + 1) % tabs.len()),
                KeyCode::BackTab | KeyCode::Char('[') => {
                    self.select_tab((position + tabs.len() - 1) % tabs.len())
                }
                KeyCode::Up => {
                    self.select_space((self.space + self.spaces.len() - 1) % self.spaces.len())
                }
                KeyCode::Down => self.select_space((self.space + 1) % self.spaces.len()),
                KeyCode::Char('s') => self.split = !self.split,
                KeyCode::Char('x') => return Ok(self.close(self.panes[self.active].id)),
                KeyCode::Char('r') => self.restart(),
                KeyCode::Char('b') => self.panes[self.active].send(&[2])?,
                KeyCode::Char(ch @ '1'..='8') => self.select_tab(ch as usize - '1' as usize),
                _ => self.notice = "Unknown muxer command; Ctrl+b shows shortcuts".into(),
            }
        } else if key.code == KeyCode::Char('b') && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.prefix = true;
        } else {
            self.panes[self.active].send(&encode(key))?;
        }
        Ok(false)
    }
    pub fn visible(&self, area: Rect) -> Vec<(usize, Rect)> {
        let content = content(area);
        let body = Rect::new(
            content.x,
            4,
            content.width,
            content.height.saturating_sub(6),
        );
        let tabs = &self.spaces[self.space].tabs;
        if self.split && tabs.len() > 1 {
            let selected = tabs
                .iter()
                .position(|id| *id == self.panes[self.active].id)
                .unwrap();
            let next = self
                .panes
                .iter()
                .position(|pane| pane.id == tabs[(selected + 1) % tabs.len()])
                .unwrap();
            let [left, _, right] = Layout::horizontal([
                Constraint::Percentage(50),
                Constraint::Length(1),
                Constraint::Min(0),
            ])
            .areas(body);
            vec![(self.active, left), (next, right)]
        } else {
            vec![(self.active, body)]
        }
    }
    fn tabs(&self, area: Rect) -> Vec<(u64, Rect)> {
        let content = content(area);
        let available = content.width.saturating_sub(8);
        let tabs = &self.spaces[self.space].tabs;
        let count = tabs.len().min(usize::from((available / 14).max(1)));
        let selected = tabs
            .iter()
            .position(|id| *id == self.panes[self.active].id)
            .unwrap();
        let start = selected.saturating_sub(count.saturating_sub(1));
        let width = (available / count as u16).min(24);
        tabs.iter()
            .skip(start)
            .take(count)
            .enumerate()
            .map(|(index, id)| {
                (
                    *id,
                    Rect::new(content.x + index as u16 * width, 2, width, 2),
                )
            })
            .collect()
    }
    pub fn click(&mut self, area: Rect, x: u16, y: u16) -> Result<bool, Box<dyn Error>> {
        let point = (x, y).into();
        if self.workspace.is_some() {
            let modal = modal(area);
            if y == modal.y + 4 {
                if (modal.x + 2..modal.x + 12).contains(&x) {
                    self.submit();
                } else if (modal.x + 15..modal.x + 25).contains(&x) {
                    self.workspace = None;
                    self.notice.clear();
                }
            }
            return Ok(false);
        }
        if x < sidebar_width(area) {
            if y == 1 {
                self.workspace = Some(String::new());
            } else if y >= 3 {
                let index = self.space_start(area) + usize::from((y - 3) / 3);
                if index < self.spaces.len() {
                    self.select_space(index);
                }
            }
            return Ok(false);
        }
        for (label, rect) in toolbar(area) {
            if rect.contains(point) {
                match label {
                    "+ Space" => self.workspace = Some(String::new()),
                    "+ Tab" => {
                        if let Err(error) = self.add_tab() {
                            self.notice = error.to_string();
                        }
                    }
                    "Split" => self.split = !self.split,
                    "Restart" => self.restart(),
                    "Quit" => return Ok(true),
                    _ => {}
                }
                return Ok(false);
            }
        }
        for (id, rect) in self.tabs(area) {
            if rect.contains(point) {
                if x >= rect.right().saturating_sub(3) {
                    return Ok(self.close(id));
                }
                self.focus(self.panes.iter().position(|pane| pane.id == id).unwrap());
                return Ok(false);
            }
        }
        let content = content(area);
        if (2..4).contains(&y) && x >= content.right().saturating_sub(8) {
            if let Err(error) = self.add_tab() {
                self.notice = error.to_string();
            }
        } else {
            for (index, rect) in self.visible(area) {
                if rect.contains(point) {
                    self.focus(index);
                    break;
                }
            }
        }
        Ok(false)
    }
    fn space_start(&self, area: Rect) -> usize {
        let visible = usize::from((area.height.saturating_sub(3) / 3).max(1));
        self.space.saturating_sub(visible.saturating_sub(1))
    }
    pub fn draw(&self, frame: &mut Frame) {
        let area = frame.area();
        frame.render_widget(Block::new().style(Style::new().bg(BG).fg(TEXT)), area);
        let sidebar = Rect::new(0, 0, sidebar_width(area), area.height);
        frame.render_widget(Block::new().style(Style::new().bg(CHROME)), sidebar);
        frame.render_widget(
            Paragraph::new(" solmu / muxer").fg(ACCENT).bold(),
            Rect::new(0, 0, sidebar.width, 1),
        );
        frame.render_widget(
            Paragraph::new(" + New space").fg(MUTED),
            Rect::new(0, 1, sidebar.width, 1),
        );
        let start = self.space_start(area);
        for (index, space) in self.spaces.iter().enumerate().skip(start) {
            let y = 3 + (index - start) as u16 * 3;
            let selected = self.space == index;
            let title = space
                .directory
                .file_name()
                .unwrap_or(space.directory.as_os_str())
                .to_string_lossy();
            let states: Vec<String> = space
                .tabs
                .iter()
                .map(|id| {
                    self.panes
                        .iter()
                        .find(|pane| pane.id == *id)
                        .expect("tab exists")
                        .state()
                })
                .collect();
            let working = states
                .iter()
                .filter(|state| state.as_str() == "working")
                .count();
            let status = if working > 0 {
                format!("{working} working")
            } else if states.iter().all(|state| state.starts_with("exited")) {
                "exited".into()
            } else if states.iter().any(|state| state == "error") {
                "error".into()
            } else if states.iter().any(|state| state == "starting") {
                "starting".into()
            } else {
                "ready".into()
            };
            frame.render_widget(
                Paragraph::new(vec![
                    Line::styled(
                        format!(
                            " {} {} {title}",
                            if selected { "›" } else { " " },
                            index + 1
                        ),
                        Style::new().fg(if selected { ACCENT } else { TEXT }).bold(),
                    ),
                    Line::from(format!(
                        "     {} tab{} · {status}",
                        space.tabs.len(),
                        if space.tabs.len() == 1 { "" } else { "s" }
                    ))
                    .fg(MUTED),
                ])
                .bg(if selected { SELECTED } else { CHROME }),
                Rect::new(0, y, sidebar.width, 3.min(area.height.saturating_sub(y))),
            );
        }
        let content = content(area);
        frame.render_widget(
            Paragraph::new(format!(" {}", self.spaces[self.space].directory.display()))
                .fg(MUTED)
                .bg(CHROME),
            Rect::new(content.x, 0, content.width, 1),
        );
        frame.render_widget(
            Block::new().bg(CHROME),
            Rect::new(content.x, 1, content.width, 1),
        );
        for (label, rect) in toolbar(area) {
            frame.render_widget(
                Paragraph::new(format!(" {label} ")).fg(ACCENT).bg(CHROME),
                rect,
            );
        }
        frame.render_widget(
            Paragraph::new(format!("Tabs: {}", self.spaces[self.space].tabs.len()))
                .fg(MUTED)
                .bg(CHROME)
                .alignment(ratatui::layout::Alignment::Right),
            Rect::new(
                content.right().saturating_sub(8),
                1,
                8.min(content.width),
                1,
            ),
        );
        for (id, rect) in self.tabs(area) {
            let selected = id == self.panes[self.active].id;
            let pane = self.panes.iter().find(|pane| pane.id == id).unwrap();
            let state = pane.state();
            let dot = if state == "working" {
                "●"
            } else if state == "error" || pane.exited.is_some() {
                "!"
            } else {
                "○"
            };
            frame.render_widget(
                Paragraph::new(format!(
                    "{} Solmu {id} {dot}",
                    if selected { "›" } else { " " }
                ))
                .fg(if selected { ACCENT } else { MUTED })
                .bg(if selected { SELECTED } else { CHROME }),
                rect,
            );
            if rect.width >= 3 {
                frame.render_widget(
                    Paragraph::new("×").fg(MUTED),
                    Rect::new(rect.right() - 3, rect.y, 3, 1),
                );
            }
        }
        frame.render_widget(
            Paragraph::new(" + Tab").fg(ACCENT),
            Rect::new(
                content.right().saturating_sub(8),
                2,
                8.min(content.width),
                2,
            ),
        );
        for (index, rect) in self.visible(area) {
            let pane = &self.panes[index];
            frame.render_widget(
                Paragraph::new(format!(" Solmu {} · {}", pane.id, pane.state()))
                    .fg(if index == self.active { ACCENT } else { MUTED }),
                Rect::new(rect.x, rect.y, rect.width, 1),
            );
            let inner = pane_inner(rect);
            let parser = pane
                .parser
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            frame.render_widget(TerminalScreen(parser.screen()), inner);
            if index == self.active && self.workspace.is_none() && pane.exited.is_none() {
                let (row, col) = parser.screen().cursor_position();
                if row < inner.height && col < inner.width {
                    frame.set_cursor_position((inner.x + col, inner.y + row));
                }
            }
        }
        let text = if self.prefix {
            "n new tab · w new space · ↑/↓ space · Tab/]/[ tab · 1-8 select · s split · x close · r restart · q quit"
        } else if !self.notice.is_empty() {
            &self.notice
        } else {
            "Ctrl+b shortcuts · click spaces and tabs · /exit leaves a tab ready to restart"
        };
        frame.render_widget(
            Paragraph::new(text).fg(MUTED).wrap(Wrap { trim: false }),
            Rect::new(content.x, area.bottom().saturating_sub(2), content.width, 2),
        );
        if let Some(path) = &self.workspace {
            let modal = modal(area);
            frame.render_widget(Clear, modal);
            frame.render_widget(
                Paragraph::new(format!(
                    "Workspace path: {path}\nEnter creates a space · Esc cancels\n{}",
                    self.notice
                ))
                .block(
                    Block::bordered()
                        .title(" New workspace ")
                        .bg(CHROME)
                        .fg(TEXT),
                )
                .wrap(Wrap { trim: false }),
                modal,
            );
            frame.render_widget(
                Paragraph::new(" Create ").fg(ACCENT),
                Rect::new(modal.x + 2, modal.y + 4, 10, 1),
            );
            frame.render_widget(
                Paragraph::new(" Cancel ").fg(MUTED),
                Rect::new(modal.x + 15, modal.y + 4, 10, 1),
            );
        }
    }
}
fn sidebar_width(area: Rect) -> u16 {
    26.min(area.width / 3)
}
fn content(area: Rect) -> Rect {
    Rect::new(
        sidebar_width(area),
        0,
        area.width.saturating_sub(sidebar_width(area)),
        area.height,
    )
}
pub fn pane_inner(rect: Rect) -> Rect {
    Rect::new(
        rect.x + 1,
        rect.y + 1,
        rect.width.saturating_sub(2),
        rect.height.saturating_sub(1),
    )
}
fn modal(area: Rect) -> Rect {
    Rect::new(
        area.width / 8,
        area.height / 3,
        area.width * 3 / 4,
        6.min(area.height),
    )
}
fn toolbar(area: Rect) -> Vec<(&'static str, Rect)> {
    let content = content(area);
    let mut x = content.x;
    ["+ Space", "+ Tab", "Split", "Restart", "Quit"]
        .into_iter()
        .filter_map(|label| {
            let width = label.len() as u16 + 4;
            if x + width > content.right().saturating_sub(10) {
                return None;
            }
            let rect = Rect::new(x, 1, width, 1);
            x += width;
            Some((label, rect))
        })
        .collect()
}
