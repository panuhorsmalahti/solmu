use crate::{
    editor::Editor,
    keys::encode,
    layout::{Axis, Divider, Node},
    pane::{Pane, TerminalScreen},
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style, Stylize},
    text::Line,
    widgets::{Block, Clear, Paragraph, Wrap},
};
use serde::{Deserialize, Serialize};
use std::{error::Error, path::PathBuf};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

const BG: Color = Color::Rgb(21, 26, 35);
const CHROME: Color = Color::Rgb(30, 37, 48);
const SELECTED: Color = Color::Rgb(45, 60, 65);
const TEXT: Color = Color::Rgb(224, 222, 244);
const MUTED: Color = Color::Rgb(136, 149, 166);
const ACCENT: Color = Color::Rgb(156, 207, 176);

#[derive(Clone, Serialize, Deserialize)]
struct Tab {
    id: u64,
    #[serde(default)]
    name: Option<String>,
    layout: Node,
    selected: u64,
    zoomed: bool,
}
#[derive(Clone, Serialize, Deserialize)]
struct Space {
    #[serde(default)]
    id: u64,
    #[serde(default)]
    name: Option<String>,
    directory: PathBuf,
    tabs: Vec<Tab>,
    selected: u64,
}

#[derive(Serialize, Deserialize)]
pub struct Snapshot {
    pub version: u32,
    next_id: u64,
    active: u64,
    spaces: Vec<Space>,
    panes: Vec<SavedPane>,
}
#[derive(Serialize, Deserialize)]
struct SavedPane {
    id: u64,
    #[serde(default)]
    name: Option<String>,
    directory: PathBuf,
    thread: Option<String>,
    exited: Option<String>,
}
impl Snapshot {
    pub fn validate(&self) -> Result<(), Box<dyn Error>> {
        if self.version != 1 {
            return Err("Saved session requires a different Muxer version".into());
        }
        if self.spaces.len() > 8 || self.panes.len() > 512 {
            return Err("Saved session exceeds supported space or pane limits".into());
        }
        let mut owned = std::collections::BTreeSet::new();
        let mut tabs = std::collections::BTreeSet::new();
        let mut spaces = std::collections::BTreeSet::new();
        for space in &self.spaces {
            if (!space.tabs.is_empty()
                && !spaces.insert(if space.id == 0 {
                    space.tabs[0].id
                } else {
                    space.id
                }))
                || !valid_name(&space.name)
                || space.tabs.is_empty()
                || space.tabs.len() > 8
                || !space.tabs.iter().any(|tab| tab.id == space.selected)
            {
                return Err("Invalid saved space selection".into());
            }
            for tab in &space.tabs {
                let ids = tab.layout.ids();
                if tab.id == 0
                    || !valid_name(&tab.name)
                    || !tabs.insert(tab.id)
                    || ids.len() > 8
                    || !tab.layout.valid()
                    || !ids.contains(&tab.selected)
                    || ids.iter().any(|id| *id == 0 || !owned.insert(*id))
                {
                    return Err("Invalid saved pane layout".into());
                }
            }
        }
        let mut recorded = std::collections::BTreeSet::new();
        for pane in &self.panes {
            if !recorded.insert(pane.id)
                || !valid_name(&pane.name)
                || pane
                    .thread
                    .as_ref()
                    .is_some_and(|thread| uuid::Uuid::parse_str(thread).is_err())
            {
                return Err("Invalid saved pane metadata".into());
            }
        }
        if recorded != owned
            || (owned.is_empty() && self.active != 0)
            || (!owned.is_empty() && !owned.contains(&self.active))
            || self.next_id == 0
            || owned
                .iter()
                .chain(tabs.iter())
                .chain(spaces.iter())
                .any(|id| *id >= self.next_id)
        {
            return Err("Invalid saved pane identity or selection".into());
        }
        Ok(())
    }
    pub fn is_empty(&self) -> bool {
        self.panes.is_empty()
    }
}
#[derive(Clone)]
struct Menu {
    area: Rect,
    selected: usize,
    target: Target,
}
#[derive(Clone, Copy)]
enum Target {
    Space(u64),
    Tab(u64),
    Pane(u64),
}
impl Target {
    fn title(self) -> &'static str {
        match self {
            Self::Space(_) => "Space",
            Self::Tab(_) => "Tab",
            Self::Pane(_) => "Pane",
        }
    }
    fn actions(self) -> &'static [&'static str] {
        match self {
            Self::Space(_) => &["Rename space", "New tab", "Close space"],
            Self::Tab(_) => &["Rename tab", "Split right", "Split down", "Close tab"],
            Self::Pane(_) => ACTIONS,
        }
    }
}
#[derive(Clone)]
struct Rename {
    target: Target,
    editor: Editor,
}
#[derive(Clone)]
struct Picker {
    editor: Editor,
    selected: usize,
    help: bool,
}
struct Entry {
    target: Target,
    label: String,
    detail: String,
}
const ACTIONS: &[&str] = &[
    "Split right",
    "Split down",
    "Zoom / restore",
    "Resize with keys",
    "Close pane",
    "Restart exited pane",
    "Rename pane",
];
pub struct App {
    pub persistent: bool,
    pub panes: Vec<Pane>,
    pub active: usize,
    workspace: Option<Editor>,
    rename: Option<Rename>,
    picker: Option<Picker>,
    navigation: bool,
    spaces: Vec<Space>,
    space: usize,
    next_id: u64,
    executable: PathBuf,
    prefix: bool,
    notice: String,
    resizing: bool,
    dragging: Option<Divider>,
    menu: Option<Menu>,
    area: Rect,
}

// Only presentation and navigation live in a client view. Layouts and real
// processes remain shared in App, owned by the background server.
#[derive(Clone)]
pub struct View {
    active: u64,
    selections: Vec<(PathBuf, u64)>,
    tabs: Vec<(u64, u64, bool)>,
    workspace: Option<Editor>,
    rename: Option<Rename>,
    picker: Option<Picker>,
    navigation: bool,
    prefix: bool,
    notice: String,
    resizing: bool,
    dragging: Option<Divider>,
    menu: Option<Menu>,
    area: Rect,
}
impl App {
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            version: 1,
            next_id: self.next_id,
            active: self.panes.get(self.active).map_or(0, |pane| pane.id),
            spaces: if self.panes.is_empty() {
                vec![]
            } else {
                self.spaces.clone()
            },
            panes: self
                .panes
                .iter()
                .map(|pane| SavedPane {
                    id: pane.id,
                    name: pane.name.clone(),
                    directory: pane.directory.clone(),
                    thread: pane
                        .parser
                        .lock()
                        .unwrap_or_else(|error| error.into_inner())
                        .callbacks()
                        .thread
                        .clone(),
                    exited: pane.exited.clone(),
                })
                .collect(),
        }
    }
    pub fn restore(executable: PathBuf, snapshot: Snapshot) -> Result<Self, Box<dyn Error>> {
        snapshot.validate()?;
        let mut app = Self::new(executable);
        app.next_id = snapshot.next_id;
        app.spaces = snapshot.spaces;
        for space in &mut app.spaces {
            if space.id == 0 {
                space.id = space.tabs[0].id;
            }
        }
        for saved in snapshot.panes {
            let mut pane = if let Some(reason) = saved.exited {
                Pane::closed(saved.id, saved.directory, saved.thread, reason)
            } else if !saved.directory.is_dir() {
                Pane::closed(
                    saved.id,
                    saved.directory,
                    saved.thread,
                    "workspace unavailable".into(),
                )
            } else {
                match Pane::resume(
                    saved.id,
                    saved.directory.clone(),
                    &app.executable,
                    saved.thread.clone(),
                ) {
                    Ok(pane) => pane,
                    Err(error) => Pane::closed(
                        saved.id,
                        saved.directory,
                        saved.thread,
                        format!("launch failed: {error}"),
                    ),
                }
            };
            pane.name = saved.name;
            app.panes.push(pane);
        }
        if let Some(index) = app.panes.iter().position(|pane| pane.id == snapshot.active) {
            app.focus(index);
        }
        Ok(app)
    }
    pub fn view(&self) -> View {
        View {
            active: self.panes[self.active].id,
            selections: self
                .spaces
                .iter()
                .map(|space| (space.directory.clone(), space.selected))
                .collect(),
            tabs: self
                .spaces
                .iter()
                .flat_map(|space| {
                    space
                        .tabs
                        .iter()
                        .map(|tab| (tab.id, tab.selected, tab.zoomed))
                })
                .collect(),
            workspace: self.workspace.clone(),
            rename: self.rename.clone(),
            picker: self.picker.clone(),
            navigation: self.navigation,
            prefix: self.prefix,
            notice: self.notice.clone(),
            resizing: self.resizing,
            dragging: self.dragging.clone(),
            menu: self.menu.clone(),
            area: self.area,
        }
    }
    pub fn use_view(&mut self, view: &View) {
        for space in &mut self.spaces {
            space.selected = view
                .selections
                .iter()
                .find(|(directory, id)| {
                    *directory == space.directory && space.tabs.iter().any(|tab| tab.id == *id)
                })
                .map(|(_, id)| *id)
                .unwrap_or(space.tabs[0].id);
            for tab in &mut space.tabs {
                if let Some((_, selected, zoomed)) =
                    view.tabs.iter().find(|(id, _, _)| *id == tab.id)
                {
                    tab.selected = if tab.layout.contains(*selected) {
                        *selected
                    } else {
                        tab.layout.ids()[0]
                    };
                    tab.zoomed = *zoomed;
                } else {
                    tab.selected = tab.layout.ids()[0];
                    tab.zoomed = false;
                }
            }
        }
        let active = self
            .panes
            .iter()
            .position(|pane| pane.id == view.active)
            .unwrap_or(0);
        self.focus(active);
        self.workspace = view.workspace.clone();
        self.rename = view.rename.clone();
        self.picker = view.picker.clone();
        self.navigation = view.navigation;
        self.prefix = view.prefix;
        self.notice = view.notice.clone();
        self.resizing = view.resizing;
        self.dragging = view.dragging.clone();
        self.menu = view.menu.clone();
        self.area = view.area;
    }
    pub fn selected_tab(&self) -> u64 {
        self.tab().id
    }
    pub fn new(executable: PathBuf) -> Self {
        Self {
            panes: vec![],
            spaces: vec![],
            active: 0,
            workspace: None,
            rename: None,
            picker: None,
            navigation: false,
            space: 0,
            next_id: 1,
            executable,
            prefix: false,
            notice: String::new(),
            resizing: false,
            dragging: None,
            menu: None,
            area: Rect::new(0, 0, 120, 40),
            persistent: false,
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
            id: pane.id,
            name: None,
            directory,
            tabs: vec![Tab {
                id: pane.id,
                name: None,
                layout: Node::Pane(pane.id),
                selected: pane.id,
                zoomed: false,
            }],
            selected: pane.id,
        });
        self.panes.push(pane);
        self.space = self.spaces.len() - 1;
        self.active = self.panes.len() - 1;
        self.next_id += 1;
        self.notice.clear();
        Ok(())
    }
    fn tab(&self) -> &Tab {
        self.spaces[self.space]
            .tabs
            .iter()
            .find(|tab| tab.id == self.spaces[self.space].selected)
            .expect("selected tab exists")
    }
    fn tab_mut(&mut self) -> &mut Tab {
        let selected = self.spaces[self.space].selected;
        self.spaces[self.space]
            .tabs
            .iter_mut()
            .find(|tab| tab.id == selected)
            .expect("selected tab exists")
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
        self.spaces[self.space].tabs.push(Tab {
            id: pane.id,
            name: None,
            layout: Node::Pane(pane.id),
            selected: pane.id,
            zoomed: false,
        });
        self.spaces[self.space].selected = pane.id;
        self.panes.push(pane);
        self.active = self.panes.len() - 1;
        self.next_id += 1;
        self.notice.clear();
        Ok(())
    }
    fn split(&mut self, axis: Axis) -> Result<(), Box<dyn Error>> {
        if self.tab().layout.ids().len() >= 8 {
            return Err("Eight panes are already open in this tab".into());
        }
        let pane = Pane::start(
            self.next_id,
            self.spaces[self.space].directory.clone(),
            &self.executable,
        )?;
        let selected = self.panes[self.active].id;
        self.tab_mut().layout.split(selected, pane.id, axis);
        self.tab_mut().selected = pane.id;
        self.tab_mut().zoomed = false;
        self.panes.push(pane);
        self.active = self.panes.len() - 1;
        self.next_id += 1;
        self.notice.clear();
        Ok(())
    }
    pub fn select_space(&mut self, index: usize) {
        self.space = index;
        let selected = self.tab().selected;
        self.active = self
            .panes
            .iter()
            .position(|pane| pane.id == selected)
            .expect("selected pane exists");
        self.resizing = false;
        self.dragging = None;
    }
    fn focus(&mut self, index: usize) {
        let id = self.panes[index].id;
        self.active = index;
        self.space = self
            .spaces
            .iter()
            .position(|space| space.tabs.iter().any(|tab| tab.layout.contains(id)))
            .expect("pane owns a space");
        let tab = self.spaces[self.space]
            .tabs
            .iter_mut()
            .find(|tab| tab.layout.contains(id))
            .expect("pane owns a tab");
        tab.selected = id;
        self.spaces[self.space].selected = tab.id;
    }
    fn select_tab(&mut self, position: usize) {
        if let Some(tab) = self.spaces[self.space].tabs.get(position) {
            let id = tab.selected;
            let index = self.panes.iter().position(|pane| pane.id == id).unwrap();
            self.focus(index);
            self.resizing = false;
            self.dragging = None;
        }
    }
    fn close_tab(&mut self, id: u64) -> bool {
        let space = self
            .spaces
            .iter()
            .position(|space| space.tabs.iter().any(|tab| tab.id == id))
            .unwrap();
        let position = self.spaces[space]
            .tabs
            .iter()
            .position(|tab| tab.id == id)
            .unwrap();
        let removed = self.spaces[space].tabs.remove(position);
        let ids = removed.layout.ids();
        self.panes.retain(|pane| !ids.contains(&pane.id));
        if self.panes.is_empty() {
            return true;
        }
        if self.spaces[space].tabs.is_empty() {
            self.spaces.remove(space);
            self.space = self.space.min(self.spaces.len() - 1);
        } else if self.spaces[space].selected == id {
            self.spaces[space].selected =
                self.spaces[space].tabs[position.min(self.spaces[space].tabs.len() - 1)].id;
        }
        self.select_space(self.space);
        false
    }
    fn close_pane(&mut self) -> bool {
        let id = self.panes[self.active].id;
        let Some(layout) = self.tab().layout.clone().remove(id) else {
            return self.close_tab(self.tab().id);
        };
        let next = layout.ids()[0];
        self.tab_mut().layout = layout;
        self.tab_mut().selected = next;
        self.panes.remove(self.active);
        self.active = self.panes.iter().position(|pane| pane.id == next).unwrap();
        self.tab_mut().zoomed = false;
        self.resizing = false;
        self.dragging = None;
        false
    }
    fn submit(&mut self) {
        if let Some(path) = self.workspace.clone() {
            match self.add_space(path.text.into()) {
                Ok(()) => self.workspace = None,
                Err(error) => self.notice = format!("Workspace unavailable: {error}"),
            }
        }
    }
    fn restart(&mut self) {
        let old = &self.panes[self.active];
        if old.exited.is_some() {
            match Pane::start(old.id, old.directory.clone(), &self.executable) {
                Ok(mut pane) => {
                    pane.name = old.name.clone();
                    self.panes[self.active] = pane;
                }
                Err(error) => self.notice = error.to_string(),
            }
        }
    }
    fn neighbor(&self, direction: KeyCode) -> Option<u64> {
        let regions = self.tab().layout.regions(body(self.area));
        let selected = regions
            .iter()
            .find(|(id, _)| *id == self.panes[self.active].id)?
            .1;
        let x = i32::from(selected.x) * 2 + i32::from(selected.width);
        let y = i32::from(selected.y) * 2 + i32::from(selected.height);
        regions
            .into_iter()
            .filter(|(id, _)| *id != self.panes[self.active].id)
            .filter_map(|(id, rect)| {
                let dx = i32::from(rect.x) * 2 + i32::from(rect.width) - x;
                let dy = i32::from(rect.y) * 2 + i32::from(rect.height) - y;
                let valid = match direction {
                    KeyCode::Left => dx < 0,
                    KeyCode::Right => dx > 0,
                    KeyCode::Up => dy < 0,
                    KeyCode::Down => dy > 0,
                    _ => false,
                };
                valid.then_some((id, dx.abs() + dy.abs()))
            })
            .min_by_key(|(_, distance)| *distance)
            .map(|(id, _)| id)
    }
    fn move_focus(&mut self, direction: KeyCode, swap: bool) {
        if let Some(next) = self.neighbor(direction) {
            if swap {
                let selected = self.panes[self.active].id;
                self.tab_mut().layout.swap(selected, next);
            } else {
                let index = self.panes.iter().position(|pane| pane.id == next).unwrap();
                self.focus(index);
            }
        }
    }
    fn direction(key: KeyCode) -> Option<KeyCode> {
        match key {
            KeyCode::Char('h' | 'H') | KeyCode::Left => Some(KeyCode::Left),
            KeyCode::Char('j' | 'J') => Some(KeyCode::Down),
            KeyCode::Char('k' | 'K') => Some(KeyCode::Up),
            KeyCode::Char('l' | 'L') | KeyCode::Right => Some(KeyCode::Right),
            _ => None,
        }
    }
    fn action(&mut self, label: &str) -> bool {
        self.menu = None;
        match label {
            "Split right" | "Split down" => {
                if let Err(error) = self.split(if label == "Split right" {
                    Axis::Right
                } else {
                    Axis::Down
                }) {
                    self.notice = error.to_string();
                }
            }
            "Zoom / restore" => self.tab_mut().zoomed = !self.tab().zoomed,
            "Resize with keys" => {
                self.resizing = true;
                self.tab_mut().zoomed = false;
            }
            "Close pane" => return self.close_pane(),
            "Restart exited pane" => self.restart(),
            "Rename pane" => self.begin_rename(Target::Pane(self.panes[self.active].id)),
            "Rename tab" => self.begin_rename(Target::Tab(self.tab().id)),
            "Rename space" => self.begin_rename(Target::Space(self.spaces[self.space].id)),
            "Close tab" => return self.close_tab(self.tab().id),
            "Close space" => {
                let ids: Vec<_> = self.spaces[self.space]
                    .tabs
                    .iter()
                    .map(|tab| tab.id)
                    .collect();
                for id in ids {
                    if self.close_tab(id) {
                        return true;
                    }
                }
            }
            "New tab" => {
                if let Err(error) = self.add_tab() {
                    self.notice = error.to_string();
                }
            }
            _ => {}
        }
        false
    }
    fn target_name(&self, target: Target) -> Option<&Option<String>> {
        match target {
            Target::Space(id) => self
                .spaces
                .iter()
                .find(|space| space.id == id)
                .map(|space| &space.name),
            Target::Tab(id) => self
                .spaces
                .iter()
                .flat_map(|space| &space.tabs)
                .find(|tab| tab.id == id)
                .map(|tab| &tab.name),
            Target::Pane(id) => self
                .panes
                .iter()
                .find(|pane| pane.id == id)
                .map(|pane| &pane.name),
        }
    }
    fn focus_target(&mut self, target: Target) -> bool {
        let pane = match target {
            Target::Space(id) => self
                .spaces
                .iter()
                .find(|space| space.id == id)
                .and_then(|space| space.tabs.iter().find(|tab| tab.id == space.selected))
                .map(|tab| tab.selected),
            Target::Tab(id) => self
                .spaces
                .iter()
                .flat_map(|space| &space.tabs)
                .find(|tab| tab.id == id)
                .map(|tab| tab.selected),
            Target::Pane(id) => Some(id),
        };
        if let Some(index) = pane.and_then(|id| self.panes.iter().position(|pane| pane.id == id)) {
            self.focus(index);
            self.resizing = false;
            self.dragging = None;
            true
        } else {
            self.notice = "That item was closed in another terminal".into();
            false
        }
    }
    fn begin_rename(&mut self, target: Target) {
        if let Some(name) = self.target_name(target) {
            self.rename = Some(Rename {
                target,
                editor: Editor::new(name.clone().unwrap_or_default()),
            });
            self.notice.clear();
        }
    }
    fn save_rename(&mut self) {
        let Some(rename) = self.rename.clone() else {
            return;
        };
        let name = rename.editor.text.trim();
        let name = (!name.is_empty()).then(|| name.to_owned());
        if !valid_name(&name) {
            self.notice = "Use at most 80 characters for a name".into();
            return;
        }
        if self.target_name(rename.target).is_none() {
            self.notice = "That item was closed in another terminal".into();
            self.rename = None;
            return;
        }
        match rename.target {
            Target::Space(id) => {
                self.spaces
                    .iter_mut()
                    .find(|space| space.id == id)
                    .unwrap()
                    .name = name
            }
            Target::Tab(id) => {
                self.spaces
                    .iter_mut()
                    .flat_map(|space| &mut space.tabs)
                    .find(|tab| tab.id == id)
                    .unwrap()
                    .name = name
            }
            Target::Pane(id) => {
                self.panes
                    .iter_mut()
                    .find(|pane| pane.id == id)
                    .unwrap()
                    .name = name
            }
        }
        self.rename = None;
        self.notice.clear();
    }
    fn open_picker(&mut self, help: bool) {
        self.menu = None;
        self.picker = Some(Picker {
            editor: Editor::default(),
            selected: 0,
            help,
        });
        self.notice.clear();
    }
    fn entries(&self, filter: &str) -> Vec<Entry> {
        let filter = filter.to_lowercase();
        let mut entries = vec![];
        for space in &self.spaces {
            let space_name = space_title(space);
            let path = space.directory.display().to_string();
            entries.push(Entry {
                target: Target::Space(space.id),
                label: format!("Space · {space_name}"),
                detail: path.clone(),
            });
            for tab in &space.tabs {
                let tab_name = tab
                    .name
                    .clone()
                    .unwrap_or_else(|| format!("Solmu {}", tab.id));
                let detail = format!("{space_name} · {path}");
                entries.push(Entry {
                    target: Target::Tab(tab.id),
                    label: format!("  Tab · {tab_name}"),
                    detail: detail.clone(),
                });
                for id in tab.layout.ids() {
                    let pane = self.panes.iter().find(|pane| pane.id == id).unwrap();
                    entries.push(Entry {
                        target: Target::Pane(id),
                        label: format!("    Pane · {} · {}", pane_title(pane), pane.state()),
                        detail: format!("{tab_name} · {detail}"),
                    });
                }
            }
        }
        entries
            .retain(|entry| matches_filter(&format!("{} {}", entry.label, entry.detail), &filter));
        entries
    }
    fn select_entry(&mut self, index: usize) {
        if let Some(picker) = &self.picker {
            if picker.help {
                self.picker = None;
                return;
            }
            let target = self
                .entries(&picker.editor.text)
                .get(index)
                .map(|entry| entry.target);
            if let Some(target) = target {
                self.focus_target(target);
                self.picker = None;
            }
        }
    }
    fn menu_action(&mut self, index: usize) -> bool {
        let Some(menu) = self.menu.take() else {
            return false;
        };
        if !self.focus_target(menu.target) {
            return false;
        }
        menu.target
            .actions()
            .get(index)
            .is_some_and(|label| self.action(label))
    }
    pub fn paste(&mut self, text: &str) -> Result<(), Box<dyn Error>> {
        if let Some(editor) = &mut self.workspace {
            editor.insert(text);
        } else if let Some(rename) = &mut self.rename {
            rename.editor.insert(text);
        } else if let Some(picker) = &mut self.picker {
            picker.editor.insert(text);
            picker.selected = 0;
        } else if self.menu.is_none() && !self.navigation && !self.prefix && !self.resizing {
            self.panes[self.active].send(text.as_bytes())?;
        }
        Ok(())
    }
    pub fn key(&mut self, key: KeyEvent) -> Result<bool, Box<dyn Error>> {
        if let Some(path) = &mut self.workspace {
            match key.code {
                KeyCode::Esc => {
                    self.workspace = None;
                    self.notice.clear();
                }
                KeyCode::Enter => self.submit(),
                _ => path.key(key),
            }
            return Ok(false);
        }
        if let Some(rename) = &mut self.rename {
            match key.code {
                KeyCode::Esc => {
                    self.rename = None;
                    self.notice.clear();
                }
                KeyCode::Enter => self.save_rename(),
                _ => rename.editor.key(key),
            }
            return Ok(false);
        }
        if let Some(picker) = &self.picker {
            let count = if picker.help {
                help_rows(&picker.editor.text).len()
            } else {
                self.entries(&picker.editor.text).len()
            };
            let picker = self.picker.as_mut().unwrap();
            picker.selected = picker.selected.min(count.saturating_sub(1));
            match key.code {
                KeyCode::Esc => self.picker = None,
                KeyCode::Up | KeyCode::BackTab => {
                    picker.selected = (picker.selected + count.saturating_sub(1)) % count.max(1)
                }
                KeyCode::Down | KeyCode::Tab => {
                    picker.selected = (picker.selected + 1) % count.max(1)
                }
                KeyCode::Enter => {
                    let selected = picker.selected;
                    self.select_entry(selected);
                }
                _ => {
                    picker.editor.key(key);
                    picker.selected = 0;
                }
            }
            return Ok(false);
        }
        if let Some(menu) = &mut self.menu {
            let count = menu.target.actions().len();
            match key.code {
                KeyCode::Esc => self.menu = None,
                KeyCode::Up => menu.selected = (menu.selected + count - 1) % count,
                KeyCode::Down => menu.selected = (menu.selected + 1) % count,
                KeyCode::Enter => {
                    let selected = menu.selected;
                    return Ok(self.menu_action(selected));
                }
                _ => {}
            }
            return Ok(false);
        }
        if self.resizing {
            if matches!(key.code, KeyCode::Enter | KeyCode::Esc) {
                self.resizing = false;
                return Ok(false);
            }
            let direction = Self::direction(key.code).unwrap_or(key.code);
            let (axis, delta) = match direction {
                KeyCode::Left => (Axis::Right, -50),
                KeyCode::Right => (Axis::Right, 50),
                KeyCode::Up => (Axis::Down, -50),
                KeyCode::Down => (Axis::Down, 50),
                _ => return Ok(false),
            };
            let id = self.panes[self.active].id;
            self.tab_mut().layout.resize_near(id, axis, delta);
            return Ok(false);
        }
        if self.navigation && !self.prefix {
            if matches!(
                key.code,
                KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q' | 'm')
            ) {
                self.navigation = false;
                return Ok(false);
            }
            if key.code != KeyCode::Char('b') || !key.modifiers.contains(KeyModifiers::CONTROL) {
                self.prefix = true;
                return self.key(key);
            }
        }
        if self.prefix {
            self.prefix = false;
            self.notice.clear();
            let position = self.spaces[self.space]
                .tabs
                .iter()
                .position(|tab| tab.id == self.spaces[self.space].selected)
                .unwrap();
            let count = self.spaces[self.space].tabs.len();
            if let Some(direction) = Self::direction(key.code) {
                let swap = key.modifiers.contains(KeyModifiers::SHIFT)
                    || matches!(key.code, KeyCode::Char('H' | 'J' | 'K' | 'L'));
                self.move_focus(direction, swap);
                return Ok(false);
            }
            match key.code {
                KeyCode::Char('q') => return Ok(true),
                KeyCode::Char('n' | 'c') => {
                    if let Err(error) = self.add_tab() {
                        self.notice = error.to_string();
                    }
                }
                KeyCode::Char('w') => self.workspace = Some(Editor::default()),
                KeyCode::Char('W') => self.begin_rename(Target::Space(self.spaces[self.space].id)),
                KeyCode::Char('T') => self.begin_rename(Target::Tab(self.tab().id)),
                KeyCode::Char('P') => self.begin_rename(Target::Pane(self.panes[self.active].id)),
                KeyCode::Char('D') => return Ok(self.action("Close space")),
                KeyCode::Char('g') => self.open_picker(false),
                KeyCode::Char('?') => self.open_picker(true),
                KeyCode::Char('m') => self.navigation = !self.navigation,
                KeyCode::Tab | KeyCode::Char(']') => self.select_tab((position + 1) % count),
                KeyCode::BackTab | KeyCode::Char('[' | 'p') => {
                    self.select_tab((position + count - 1) % count)
                }
                KeyCode::Up => {
                    self.select_space((self.space + self.spaces.len() - 1) % self.spaces.len())
                }
                KeyCode::Down => self.select_space((self.space + 1) % self.spaces.len()),
                KeyCode::Char('s' | 'v') => return Ok(self.action("Split right")),
                KeyCode::Char('-') => return Ok(self.action("Split down")),
                KeyCode::Char('z') => return Ok(self.action("Zoom / restore")),
                KeyCode::Char('x') => return Ok(self.close_pane()),
                KeyCode::Char('X') => return Ok(self.close_tab(self.tab().id)),
                KeyCode::Char('r') => {
                    if self.panes[self.active].exited.is_some() {
                        self.restart();
                    } else {
                        return Ok(self.action("Resize with keys"));
                    }
                }
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
        let regions = if self.tab().zoomed {
            vec![(self.panes[self.active].id, body(area))]
        } else {
            self.tab().layout.regions(body(area))
        };
        regions
            .into_iter()
            .map(|(id, rect)| {
                (
                    self.panes.iter().position(|pane| pane.id == id).unwrap(),
                    rect,
                )
            })
            .collect()
    }
    fn tabs(&self, area: Rect) -> Vec<(u64, Rect)> {
        let content = content(area);
        let available = content.width.saturating_sub(8);
        let tabs = &self.spaces[self.space].tabs;
        let count = tabs.len().min(usize::from((available / 14).max(1)));
        let selected = tabs
            .iter()
            .position(|tab| tab.id == self.spaces[self.space].selected)
            .unwrap();
        let start = selected.saturating_sub(count.saturating_sub(1));
        let width = (available / count as u16).min(24);
        tabs.iter()
            .skip(start)
            .take(count)
            .enumerate()
            .map(|(index, tab)| {
                (
                    tab.id,
                    Rect::new(content.x + index as u16 * width, 2, width, 2),
                )
            })
            .collect()
    }
    pub fn context_menu(&mut self, area: Rect, x: u16, y: u16) {
        if self.workspace.is_some() || self.rename.is_some() || self.picker.is_some() {
            return;
        }
        self.area = area;
        let target = if x < sidebar_width(area) && y >= 3 {
            let index = self.space_start(area) + usize::from((y - 3) / 3);
            let Some(space) = self.spaces.get(index) else {
                return;
            };
            Target::Space(space.id)
        } else if let Some((id, _)) = self
            .tabs(area)
            .into_iter()
            .find(|(_, rect)| rect.contains((x, y).into()))
        {
            Target::Tab(id)
        } else if let Some((index, _)) = self
            .visible(area)
            .into_iter()
            .find(|(_, rect)| rect.contains((x, y).into()))
        {
            Target::Pane(self.panes[index].id)
        } else {
            Target::Pane(self.panes[self.active].id)
        };
        self.show_menu(target, area, x, y);
    }
    fn show_menu(&mut self, target: Target, area: Rect, x: u16, y: u16) {
        self.focus_target(target);
        let width = 26.min(area.width);
        let height = (target.actions().len() as u16 + 2).min(area.height);
        self.menu = Some(Menu {
            area: Rect::new(
                x.min(area.width.saturating_sub(width)),
                y.min(area.height.saturating_sub(height)),
                width,
                height,
            ),
            selected: 0,
            target,
        });
    }
    pub fn click(&mut self, area: Rect, x: u16, y: u16) -> Result<bool, Box<dyn Error>> {
        let point = (x, y).into();
        self.area = area;
        if let Some(menu) = &self.menu {
            if menu.area.contains(point) && y > menu.area.y && y < menu.area.bottom() - 1 {
                return Ok(self.menu_action(usize::from(y - menu.area.y - 1)));
            }
            self.menu = None;
            return Ok(false);
        }
        if self.picker.is_some() {
            let rect = picker_area(area);
            if y == rect.y && x >= rect.right().saturating_sub(4) && rect.contains(point) {
                self.picker = None;
            } else if y >= rect.y + 3 && y < rect.bottom().saturating_sub(2) && rect.contains(point)
            {
                let picker = self.picker.as_ref().unwrap();
                let visible = usize::from(rect.height.saturating_sub(5));
                let count = if picker.help {
                    help_rows(&picker.editor.text).len()
                } else {
                    self.entries(&picker.editor.text).len()
                };
                let start = picker
                    .selected
                    .min(count.saturating_sub(1))
                    .saturating_sub(visible.saturating_sub(1));
                self.select_entry(start + usize::from(y - rect.y - 3));
            }
            return Ok(false);
        }
        if self.rename.is_some() {
            let rect = modal(area);
            if y == rect.y + 4 {
                if (rect.x + 2..rect.x + 12).contains(&x) {
                    self.save_rename();
                } else if (rect.x + 15..rect.x + 25).contains(&x) {
                    self.rename = None;
                    self.notice.clear();
                }
            }
            return Ok(false);
        }
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
                self.workspace = Some(Editor::default());
            } else if y >= 3 {
                let index = self.space_start(area) + usize::from((y - 3) / 3);
                if index < self.spaces.len() {
                    self.select_space(index);
                }
            }
            return Ok(false);
        }
        for (label, rect) in toolbar(area, self.persistent) {
            if rect.contains(point) {
                match label {
                    "+ Space" => self.workspace = Some(Editor::default()),
                    "Find" => self.open_picker(false),
                    "Navigate" => self.navigation = !self.navigation,
                    "Help" => self.open_picker(true),
                    "+ Tab" => {
                        if let Err(error) = self.add_tab() {
                            self.notice = error.to_string();
                        }
                    }
                    "Split" => {
                        self.show_menu(Target::Pane(self.panes[self.active].id), area, x, y + 1)
                    }
                    "Zoom" => return Ok(self.action("Zoom / restore")),
                    "Restart" => self.restart(),
                    "Quit" | "Detach" => return Ok(true),
                    _ => {}
                }
                return Ok(false);
            }
        }
        for (id, rect) in self.tabs(area) {
            if rect.contains(point) {
                if x >= rect.right().saturating_sub(3) {
                    return Ok(self.close_tab(id));
                }
                let tab = self.spaces[self.space]
                    .tabs
                    .iter()
                    .find(|tab| tab.id == id)
                    .unwrap();
                let selected = tab.selected;
                self.focus(
                    self.panes
                        .iter()
                        .position(|pane| pane.id == selected)
                        .unwrap(),
                );
                return Ok(false);
            }
        }
        if !self.tab().zoomed
            && let Some(divider) = self
                .tab()
                .layout
                .dividers(body(area))
                .into_iter()
                .find(|divider| divider.area.contains(point))
        {
            self.dragging = Some(divider);
            return Ok(false);
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
                    if y == rect.y && x >= rect.right().saturating_sub(2) {
                        return Ok(self.close_pane());
                    }
                    break;
                }
            }
        }
        Ok(false)
    }
    pub fn drag(&mut self, x: u16, y: u16) {
        if let Some(divider) = &self.dragging {
            let (offset, length) = if divider.axis == Axis::Right {
                (
                    x.saturating_sub(divider.parent.x),
                    divider.parent.width.saturating_sub(1),
                )
            } else {
                (
                    y.saturating_sub(divider.parent.y),
                    divider.parent.height.saturating_sub(1),
                )
            };
            let ratio = (u32::from(offset) * 1000 / u32::from(length.max(1))).min(1000) as u16;
            let path = divider.path.clone();
            self.tab_mut().layout.set_ratio(&path, ratio);
        }
    }
    pub fn end_drag(&mut self) {
        self.dragging = None;
    }
    pub fn resized(&mut self, area: Rect) {
        if area != self.area {
            self.dragging = None;
        }
        self.area = area;
    }
    fn space_start(&self, area: Rect) -> usize {
        let visible = usize::from((area.height.saturating_sub(3) / 3).max(1));
        self.space.saturating_sub(visible.saturating_sub(1))
    }
    pub fn draw(&self, frame: &mut Frame) {
        let area = frame.area();
        frame.render_widget(Block::new().style(Style::new().bg(BG).fg(TEXT)), area);
        let sidebar = Rect::new(0, 0, sidebar_width(area), area.height);
        frame.render_widget(Block::new().bg(CHROME), sidebar);
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
            if y >= area.height {
                break;
            }
            let selected = self.space == index;
            let title = space_title(space);
            let states: Vec<_> = space
                .tabs
                .iter()
                .flat_map(|tab| tab.layout.ids())
                .map(|id| {
                    self.panes
                        .iter()
                        .find(|pane| pane.id == id)
                        .unwrap()
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
        for (label, rect) in toolbar(area, self.persistent) {
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
            let selected = id == self.spaces[self.space].selected;
            let tab = self.spaces[self.space]
                .tabs
                .iter()
                .find(|tab| tab.id == id)
                .unwrap();
            let states: Vec<_> = tab
                .layout
                .ids()
                .iter()
                .map(|id| {
                    self.panes
                        .iter()
                        .find(|pane| pane.id == *id)
                        .unwrap()
                        .state()
                })
                .collect();
            let dot = if states.iter().any(|state| state == "working") {
                "●"
            } else if states
                .iter()
                .any(|state| state == "error" || state.starts_with("exited"))
            {
                "!"
            } else {
                "○"
            };
            frame.render_widget(
                Paragraph::new(format!(
                    "{} {} {dot}",
                    if selected { "›" } else { " " },
                    tab.name.clone().unwrap_or_else(|| format!("Solmu {id}"))
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
        if !self.tab().zoomed {
            for divider in self.tab().layout.dividers(body(area)) {
                let text = if divider.axis == Axis::Right {
                    "│\n".repeat(usize::from(divider.area.height))
                } else {
                    "─".repeat(usize::from(divider.area.width))
                };
                frame.render_widget(
                    Paragraph::new(text).fg(
                        if self
                            .dragging
                            .as_ref()
                            .is_some_and(|drag| drag.path == divider.path)
                        {
                            ACCENT
                        } else {
                            CHROME
                        },
                    ),
                    divider.area,
                );
            }
        }
        for (index, rect) in self.visible(area) {
            if rect.height == 0 || rect.width == 0 {
                continue;
            }
            let pane = &self.panes[index];
            frame.render_widget(
                Paragraph::new(format!(
                    "{}{} · {}{}",
                    if index == self.active { "› " } else { "  " },
                    pane_header_title(pane, rect.width, self.tab().zoomed),
                    pane.state(),
                    if self.tab().zoomed { " · zoomed" } else { "" }
                ))
                .fg(if index == self.active { ACCENT } else { MUTED }),
                Rect::new(rect.x, rect.y, rect.width, 1),
            );
            if rect.width >= 3 {
                frame.render_widget(
                    Paragraph::new("×").fg(MUTED),
                    Rect::new(rect.right() - 2, rect.y, 2, 1),
                );
            }
            let inner = pane_inner(rect);
            let parser = pane
                .parser
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            frame.render_widget(TerminalScreen(parser.screen()), inner);
            if index == self.active
                && self.workspace.is_none()
                && self.rename.is_none()
                && self.picker.is_none()
                && !self.navigation
                && self.menu.is_none()
                && !self.resizing
                && pane.exited.is_none()
            {
                let (row, col) = parser.screen().cursor_position();
                if row < inner.height && col < inner.width {
                    frame.set_cursor_position((inner.x + col, inner.y + row));
                }
            }
        }
        let text = if self.prefix {
            if self.persistent {
                "n new tab · w space · v split right · - split down · h/j/k/l focus · H/J/K/L swap · z zoom · r resize/restart · x close pane · q detach"
            } else {
                "n new tab · w space · v split right · - split down · h/j/k/l focus · H/J/K/L swap · z zoom · r resize/restart · x close pane · q quit"
            }
        } else if self.navigation {
            "Navigate: h/j/k/l panes · Up/Down spaces · Tab tabs · g find · ? help · Enter/Esc resumes typing"
        } else if self.resizing {
            "Resize: arrows or h/j/k/l move the nearest divider · Enter/Esc finishes · drag borders with the mouse"
        } else if !self.notice.is_empty() {
            &self.notice
        } else {
            "Ctrl+b ? help · Ctrl+b g find · right-click for actions · drag borders to resize"
        };
        frame.render_widget(
            Paragraph::new(text).fg(MUTED).wrap(Wrap { trim: false }),
            Rect::new(content.x, area.bottom().saturating_sub(2), content.width, 2),
        );
        if let Some(path) = &self.workspace {
            draw_editor(
                frame,
                area,
                "New workspace",
                "Workspace path:",
                path,
                &self.notice,
                "Create",
            );
        }
        if let Some(rename) = &self.rename {
            draw_editor(
                frame,
                area,
                &format!("Rename {}", rename.target.title().to_lowercase()),
                "Name (empty resets default):",
                &rename.editor,
                &self.notice,
                "Save",
            );
        }
        if let Some(picker) = &self.picker {
            let rect = picker_area(area);
            frame.render_widget(Clear, rect);
            frame.render_widget(
                Block::bordered()
                    .title(if picker.help {
                        " Keyboard help "
                    } else {
                        " Find spaces, tabs and panes "
                    })
                    .bg(CHROME)
                    .fg(TEXT),
                rect,
            );
            frame.render_widget(
                Paragraph::new("×").fg(MUTED).bg(CHROME),
                Rect::new(rect.right().saturating_sub(4), rect.y, 3, 1),
            );
            let (query, cursor) = picker.editor.visible(rect.width.saturating_sub(5));
            frame.render_widget(
                Paragraph::new(format!(" > {query}")).fg(ACCENT),
                Rect::new(rect.x + 1, rect.y + 1, rect.width.saturating_sub(2), 1),
            );
            let entries: Vec<_> = if picker.help {
                help_rows(&picker.editor.text)
            } else {
                self.entries(&picker.editor.text)
                    .into_iter()
                    .map(|entry| (entry.label, entry.detail))
                    .collect()
            };
            let visible = usize::from(rect.height.saturating_sub(5));
            let selected = picker.selected.min(entries.len().saturating_sub(1));
            let start = selected.saturating_sub(visible.saturating_sub(1));
            for (index, (label, detail)) in entries.iter().enumerate().skip(start).take(visible) {
                frame.render_widget(
                    Paragraph::new(format!("{label}    {detail}"))
                        .fg(if index == selected { ACCENT } else { TEXT })
                        .bg(if index == selected { SELECTED } else { CHROME }),
                    Rect::new(
                        rect.x + 2,
                        rect.y + 3 + (index - start) as u16,
                        rect.width.saturating_sub(4),
                        1,
                    ),
                );
            }
            if entries.is_empty() {
                frame.render_widget(
                    Paragraph::new("No matches").fg(MUTED),
                    Rect::new(rect.x + 2, rect.y + 3, rect.width.saturating_sub(4), 1),
                );
            }
            frame.render_widget(
                Paragraph::new(if picker.help {
                    "Type to filter · ↑/↓ scroll · Enter/Esc closes"
                } else {
                    "Type to filter · ↑/↓ select · Enter opens · Esc closes"
                })
                .fg(MUTED),
                Rect::new(
                    rect.x + 2,
                    rect.bottom().saturating_sub(2),
                    rect.width.saturating_sub(4),
                    1,
                ),
            );
            if rect.height >= 2 && rect.width >= 5 {
                frame.set_cursor_position((rect.x + 3 + cursor, rect.y + 1));
            }
        }
        if let Some(menu) = &self.menu {
            frame.render_widget(Clear, menu.area);
            frame.render_widget(
                Block::bordered()
                    .title(format!(" {} ", menu.target.title()))
                    .bg(CHROME)
                    .fg(MUTED),
                menu.area,
            );
            for (index, label) in menu.target.actions().iter().enumerate() {
                if index as u16 + 2 >= menu.area.height {
                    break;
                }
                frame.render_widget(
                    Paragraph::new(*label)
                        .fg(if index == menu.selected { ACCENT } else { TEXT })
                        .bg(if index == menu.selected {
                            SELECTED
                        } else {
                            CHROME
                        }),
                    Rect::new(
                        menu.area.x + 1,
                        menu.area.y + index as u16 + 1,
                        menu.area.width.saturating_sub(2),
                        1,
                    ),
                );
            }
        }
    }
}
fn sidebar_width(area: Rect) -> u16 {
    26.min(area.width / 3)
}
fn valid_name(name: &Option<String>) -> bool {
    name.as_ref().is_none_or(|name| {
        !name.is_empty() && name.chars().count() <= 80 && !name.chars().any(char::is_control)
    })
}
fn space_title(space: &Space) -> String {
    space.name.clone().unwrap_or_else(|| {
        space
            .directory
            .file_name()
            .unwrap_or(space.directory.as_os_str())
            .to_string_lossy()
            .into_owned()
    })
}
fn pane_title(pane: &Pane) -> String {
    pane.name.as_ref().map_or_else(
        || format!("Solmu {}", pane.id),
        |name| format!("{name} #{}", pane.id),
    )
}
fn pane_header_title(pane: &Pane, width: u16, zoomed: bool) -> String {
    if let Some(name) = &pane.name {
        let suffix = format!(
            " #{} · {}{}",
            pane.id,
            pane.state(),
            if zoomed { " · zoomed" } else { "" }
        );
        let available =
            usize::from(width).saturating_sub(UnicodeWidthStr::width(suffix.as_str()) + 4);
        let mut cells = 0;
        let clipped: String = name
            .graphemes(true)
            .take_while(|grapheme| {
                cells += UnicodeWidthStr::width(*grapheme);
                cells <= available
            })
            .collect();
        format!("{clipped} #{}", pane.id)
    } else {
        pane_title(pane)
    }
}
fn matches_filter(text: &str, filter: &str) -> bool {
    let text = text.to_lowercase();
    filter.split_whitespace().all(|part| text.contains(part))
}
fn help_rows(filter: &str) -> Vec<(String, String)> {
    [
        ("Ctrl+b n / c", "New tab"),
        ("Ctrl+b w", "New space"),
        ("Ctrl+b Up / Down", "Previous / next space"),
        ("Ctrl+b Tab / ]", "Next tab"),
        ("Ctrl+b Shift+Tab / [ / p", "Previous tab"),
        ("Ctrl+b 1–8", "Select tab by position"),
        ("Ctrl+b v / s", "Split right"),
        ("Ctrl+b -", "Split down"),
        ("Ctrl+b h/j/k/l", "Focus pane left/down/up/right"),
        ("Ctrl+b H/J/K/L", "Swap pane left/down/up/right"),
        ("Ctrl+b z", "Zoom / restore pane"),
        ("Ctrl+b r", "Resize running pane / restart exited pane"),
        ("Ctrl+b x", "Close pane"),
        ("Ctrl+b X", "Close tab"),
        ("Ctrl+b D", "Close space"),
        ("Ctrl+b W", "Rename space"),
        ("Ctrl+b T", "Rename tab"),
        ("Ctrl+b P", "Rename pane"),
        ("Ctrl+b g", "Find spaces, tabs and panes"),
        ("Ctrl+b m", "Navigation mode"),
        ("Ctrl+b ?", "Search keyboard help"),
        ("Ctrl+b q", "Detach / quit foreground session"),
        ("Ctrl+b b", "Send literal Ctrl+b"),
        (
            "Left/Right · Home/End",
            "Edit fields at cursor, by Unicode grapheme",
        ),
        ("Alt+b/f · Ctrl+Left/Right", "Move by word in fields"),
        (
            "Ctrl+u/k/w · Alt+d",
            "Cut text before/after cursor, previous/next word",
        ),
        ("Ctrl+y", "Insert last cut text in this field"),
        ("Right-click", "Space / tab / pane actions"),
        ("Drag divider", "Resize panes"),
    ]
    .into_iter()
    .filter(|(key, label)| matches_filter(&format!("{key} {label}"), &filter.to_lowercase()))
    .map(|(key, label)| (key.into(), label.into()))
    .collect()
}
fn picker_area(area: Rect) -> Rect {
    let width = area.width.saturating_sub(4).min(140);
    let height = area.height.saturating_sub(4).min(26);
    Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    )
}
fn draw_editor(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    label: &str,
    editor: &Editor,
    notice: &str,
    confirm: &str,
) {
    let rect = modal(area);
    frame.render_widget(Clear, rect);
    frame.render_widget(
        Block::bordered()
            .title(format!(" {title} "))
            .bg(CHROME)
            .fg(TEXT),
        rect,
    );
    let width = rect.width.saturating_sub(4);
    let (text, cursor) = editor.visible(width);
    frame.render_widget(
        Paragraph::new(label).fg(MUTED),
        Rect::new(rect.x + 2, rect.y + 1, width, 1),
    );
    frame.render_widget(
        Paragraph::new(text).fg(TEXT).bg(SELECTED),
        Rect::new(rect.x + 2, rect.y + 2, width, 1),
    );
    frame.render_widget(
        Paragraph::new(notice).fg(MUTED),
        Rect::new(rect.x + 2, rect.y + 3, width, 1),
    );
    frame.render_widget(
        Paragraph::new(format!(" {confirm} ")).fg(ACCENT),
        Rect::new(rect.x + 2, rect.y + 4, 10, 1),
    );
    frame.render_widget(
        Paragraph::new(" Cancel ").fg(MUTED),
        Rect::new(rect.x + 15, rect.y + 4, 10, 1),
    );
    if rect.height >= 4 && rect.width >= 5 {
        frame.set_cursor_position((rect.x + 2 + cursor, rect.y + 2));
    }
}
fn content(area: Rect) -> Rect {
    Rect::new(
        sidebar_width(area),
        0,
        area.width.saturating_sub(sidebar_width(area)),
        area.height,
    )
}
fn body(area: Rect) -> Rect {
    let content = content(area);
    Rect::new(
        content.x,
        4,
        content.width,
        content.height.saturating_sub(6),
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
fn toolbar(area: Rect, persistent: bool) -> Vec<(&'static str, Rect)> {
    let content = content(area);
    let mut x = content.x;
    [
        "+ Space",
        "+ Tab",
        "Split",
        "Zoom",
        "Restart",
        if persistent { "Detach" } else { "Quit" },
        "Find",
        "Navigate",
        "Help",
    ]
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
