use super::*;
use crate::control::{Kind, Request, Zoom, key_bytes};
use serde_json::{Value, json};

fn target(kind: Kind, id: u64) -> Target {
    match kind {
        Kind::Space => Target::Space(id),
        Kind::Tab => Target::Tab(id),
        Kind::Pane => Target::Pane(id),
    }
}
fn name(value: &Option<String>) -> Result<Option<String>, Box<dyn Error>> {
    let value = value
        .as_ref()
        .map(|v| v.trim().to_owned())
        .filter(|v| !v.is_empty());
    if !valid_name(&value) {
        return Err("Names must contain at most 80 characters without control characters".into());
    }
    Ok(value)
}
fn ratio(value: u16) -> Result<(), Box<dyn Error>> {
    if !(100..=900).contains(&value) {
        return Err("Ratio must be 100 through 900 (thousandths)".into());
    }
    Ok(())
}
fn directory(path: PathBuf) -> Result<PathBuf, Box<dyn Error>> {
    let path = path.canonicalize()?;
    if !path.is_dir() {
        return Err("Workspace must be an existing directory".into());
    }
    Ok(path)
}

impl App {
    fn automation_pane(&self, id: u64) -> Result<&Pane, Box<dyn Error>> {
        self.panes
            .iter()
            .find(|pane| pane.id == id)
            .ok_or_else(|| "Pane does not exist".into())
    }
    fn automation_focus(&mut self, kind: Kind, id: u64) -> Result<(), Box<dyn Error>> {
        if self.target_name(target(kind, id)).is_none() {
            return Err("Target does not exist".into());
        }
        self.focus_target(target(kind, id));
        Ok(())
    }
    fn automation_record(&self, kind: Kind, id: u64) -> Result<Value, Box<dyn Error>> {
        match kind {
            Kind::Space => {
                let space = self
                    .spaces
                    .iter()
                    .find(|space| space.id == id)
                    .ok_or("Space does not exist")?;
                Ok(
                    json!({"id": space.id, "name": space.name, "cwd": space.directory, "selected_tab": space.selected, "tabs": space.tabs.iter().map(|tab| tab.id).collect::<Vec<_>>() }),
                )
            }
            Kind::Tab => {
                let space = self
                    .spaces
                    .iter()
                    .find(|space| space.tabs.iter().any(|tab| tab.id == id))
                    .ok_or("Tab does not exist")?;
                let tab = space.tabs.iter().find(|tab| tab.id == id).unwrap();
                Ok(
                    json!({"id": tab.id, "space": space.id, "name": tab.name, "selected_pane": tab.selected, "zoomed": tab.zoomed, "layout": tab.layout, "panes": tab.layout.ids()}),
                )
            }
            Kind::Pane => {
                let pane = self.automation_pane(id)?;
                let space = self
                    .spaces
                    .iter()
                    .find(|space| space.tabs.iter().any(|tab| tab.layout.contains(id)))
                    .unwrap();
                let tab = space
                    .tabs
                    .iter()
                    .find(|tab| tab.layout.contains(id))
                    .unwrap();
                let parser = pane
                    .parser
                    .lock()
                    .unwrap_or_else(|error| error.into_inner());
                let (rows, cols) = parser.screen().size();
                let state = pane.exited.as_ref().unwrap_or(&parser.callbacks().label);
                Ok(
                    json!({"id": id, "space": space.id, "tab": tab.id, "name": pane.name, "cwd": pane.directory, "state": state, "thread": parser.callbacks().thread, "exited": pane.exited, "instance": pane.instance, "pid": pane.pid(), "rows": rows, "cols": cols}),
                )
            }
        }
    }
    pub fn automation_snapshot(&self) -> Value {
        json!({
            "version": 1,
            "active_pane": self.panes.get(self.active).map(|pane| pane.id),
            "active_space": self.spaces.get(self.space).map(|space| space.id),
            "active_tab": if self.panes.is_empty() { None } else { Some(self.tab().id) },
            "spaces": self.spaces.iter().map(|space| self.automation_record(Kind::Space, space.id).unwrap()).collect::<Vec<_>>(),
            "tabs": self.spaces.iter().flat_map(|space| &space.tabs).map(|tab| self.automation_record(Kind::Tab, tab.id).unwrap()).collect::<Vec<_>>(),
            "panes": self.panes.iter().map(|pane| self.automation_record(Kind::Pane, pane.id).unwrap()).collect::<Vec<_>>()
        })
    }
    pub fn automation(&mut self, request: &Request) -> Result<Value, Box<dyn Error>> {
        let before = self.view();
        let result = self.automation_inner(request);
        // A failed or non-focus operation must not steal the server's view.
        // Attached terminals have separate views, which the coordinator restores.
        if !self.panes.is_empty() && (result.is_err() || !request.changes_view()) {
            self.use_view(&before);
        }
        result
    }
    fn automation_inner(&mut self, request: &Request) -> Result<Value, Box<dyn Error>> {
        match request {
            Request::Snapshot => Ok(self.automation_snapshot()),
            Request::Get { target, id } => self.automation_record(*target, *id),
            Request::List { target, parent } => {
                let snapshot = self.automation_snapshot();
                let (key, owner) = match target {
                    Kind::Space => ("spaces", ""),
                    Kind::Tab => ("tabs", "space"),
                    Kind::Pane => ("panes", "tab"),
                };
                if let Some(id) = parent {
                    match target {
                        Kind::Space => return Err("Space lists do not accept a parent".into()),
                        Kind::Tab => {
                            self.automation_record(Kind::Space, *id)?;
                        }
                        Kind::Pane => {
                            self.automation_record(Kind::Tab, *id)?;
                        }
                    }
                }
                Ok(
                    json!({"items": snapshot[key].as_array().unwrap().iter().filter(|value| parent.is_none_or(|id| value[owner].as_u64() == Some(id))).collect::<Vec<_>>()}),
                )
            }
            Request::CreateSpace {
                cwd, name: label, ..
            } => {
                let label = name(label)?;
                let cwd = directory(cwd.clone().map_or_else(|| self.new_cwd(), Ok)?)?;
                self.add_space(cwd)?;
                self.spaces[self.space].name = label;
                self.automation_created()
            }
            Request::CreateTab {
                space,
                cwd,
                name: label,
                ..
            } => {
                let label = name(label)?;
                self.automation_focus(Kind::Space, *space)?;
                let cwd = directory(cwd.clone().map_or_else(|| self.new_cwd(), Ok)?)?;
                self.add_tab_in(cwd)?;
                self.tab_mut().name = label;
                self.automation_created()
            }
            Request::SplitPane {
                pane,
                axis,
                ratio: value,
                cwd,
                name: label,
                ..
            } => {
                let label = name(label)?;
                ratio(*value)?;
                self.automation_focus(Kind::Pane, *pane)?;
                let cwd = directory(cwd.clone().map_or_else(|| self.new_cwd(), Ok)?)?;
                self.split_in(*axis, cwd, *value)?;
                self.panes[self.active].name = label;
                self.automation_created()
            }
            Request::Focus { target, id, .. } => {
                self.automation_focus(*target, *id)?;
                self.automation_record(*target, *id)
            }
            Request::Rename {
                target: kind,
                id,
                name: label,
            } => {
                let label = name(label)?;
                self.automation_record(*kind, *id)?;
                match kind {
                    Kind::Space => {
                        self.spaces
                            .iter_mut()
                            .find(|space| space.id == *id)
                            .unwrap()
                            .name = label
                    }
                    Kind::Tab => {
                        self.spaces
                            .iter_mut()
                            .flat_map(|space| &mut space.tabs)
                            .find(|tab| tab.id == *id)
                            .unwrap()
                            .name = label
                    }
                    Kind::Pane => {
                        self.panes
                            .iter_mut()
                            .find(|pane| pane.id == *id)
                            .unwrap()
                            .name = label
                    }
                }
                self.automation_record(*kind, *id)
            }
            Request::Close { target: kind, id } => {
                self.automation_focus(*kind, *id)?;
                match kind {
                    Kind::Pane => {
                        self.close_pane();
                    }
                    Kind::Tab => {
                        self.close_tab(*id);
                    }
                    Kind::Space => {
                        self.action("Close space");
                    }
                }
                Ok(json!({"closed": id, "target": kind, "session_stopped": self.panes.is_empty()}))
            }
            Request::Swap { pane, other } => {
                self.automation_focus(Kind::Pane, *pane)?;
                if pane == other || !self.tab().layout.contains(*other) {
                    return Err("Swap requires two different panes in the same tab".into());
                }
                self.tab_mut().layout.swap(*pane, *other);
                self.automation_record(Kind::Tab, self.tab().id)
            }
            Request::Zoom { pane, mode, .. } => {
                self.automation_focus(Kind::Pane, *pane)?;
                self.tab_mut().zoomed = match mode {
                    Zoom::On => true,
                    Zoom::Off => false,
                    Zoom::Toggle => !self.tab().zoomed,
                };
                self.automation_record(Kind::Tab, self.tab().id)
            }
            Request::Resize {
                pane,
                axis,
                ratio: value,
            } => {
                ratio(*value)?;
                self.automation_focus(Kind::Pane, *pane)?;
                if !self.tab_mut().layout.set_near_ratio(*pane, *axis, *value) {
                    return Err("No divider on that axis next to this pane".into());
                }
                self.automation_record(Kind::Tab, self.tab().id)
            }
            Request::Restart { pane } => {
                self.automation_focus(Kind::Pane, *pane)?;
                let old = &self.panes[self.active];
                if old.exited.is_none() {
                    return Err("Only stopped panes can be restarted".into());
                }
                let mut fresh =
                    Pane::start(old.id, directory(old.directory.clone())?, &self.executable)?;
                fresh.name = old.name.clone();
                self.panes[self.active] = fresh;
                self.automation_record(Kind::Pane, *pane)
            }
            Request::Read { pane, lines, ansi } => {
                if lines.is_some_and(|lines| !(1..=100).contains(&lines)) {
                    return Err("Lines must be 1 through 100".into());
                }
                let pane = self.automation_pane(*pane)?;
                let parser = pane
                    .parser
                    .lock()
                    .unwrap_or_else(|error| error.into_inner());
                let screen = parser.screen();
                let rows: Vec<_> = screen.rows(0, screen.size().1).collect();
                let text = if *ansi {
                    String::from_utf8_lossy(&screen.contents_formatted()).into_owned()
                } else {
                    let start = rows
                        .len()
                        .saturating_sub(usize::from(lines.unwrap_or(screen.size().0)));
                    rows[start..].join("\n").trim_end().to_owned()
                };
                if *ansi && lines.is_some() {
                    return Err("--lines is available for plain text reads".into());
                }
                Ok(
                    json!({"pane": pane.id, "instance": pane.instance, "text": text, "rows": screen.size().0, "cols": screen.size().1, "format": if *ansi { "ansi" } else { "text" }}),
                )
            }
            Request::SendText { pane, text } => {
                if text.is_empty()
                    || text.len() > 65536
                    || text
                        .chars()
                        .any(|ch| ch.is_control() && !matches!(ch, '\n' | '\r' | '\t'))
                {
                    return Err(
                        "Text must contain 1 through 65536 bytes; use send-keys for control keys"
                            .into(),
                    );
                }
                let pane = self.automation_pane(*pane)?;
                if pane.exited.is_some() {
                    return Err("Pane is stopped".into());
                }
                let paste = pane
                    .parser
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .screen()
                    .bracketed_paste();
                if !paste && text.chars().any(|ch| matches!(ch, '\n' | '\r' | '\t')) {
                    return Err(
                        "Pane has not enabled bracketed paste; use send-keys for Tab or Enter"
                            .into(),
                    );
                }
                let bytes = if paste {
                    format!("\x1b[200~{text}\x1b[201~").into_bytes()
                } else {
                    text.as_bytes().to_vec()
                };
                pane.send(&bytes)?;
                Ok(json!({"pane": pane.id, "queued": true}))
            }
            Request::SendKeys { pane, keys } => {
                let bytes = key_bytes(keys)?;
                let pane = self.automation_pane(*pane)?;
                if pane.exited.is_some() {
                    return Err("Pane is stopped".into());
                }
                pane.send(&bytes)?;
                Ok(json!({"pane": pane.id, "queued": true}))
            }
        }
    }
    fn automation_created(&self) -> Result<Value, Box<dyn Error>> {
        Ok(
            json!({"space": self.automation_record(Kind::Space, self.spaces[self.space].id)?, "tab": self.automation_record(Kind::Tab, self.tab().id)?, "pane": self.automation_record(Kind::Pane, self.panes[self.active].id)?}),
        )
    }
}
