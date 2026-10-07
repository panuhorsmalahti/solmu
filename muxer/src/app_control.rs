use super::*;
use crate::control::{Kind, Request, Zoom, key_bytes};
use serde_json::{Value, json};
use std::process::Command;

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

fn worktree_slug(branch: &str) -> String {
    let mut slug = String::new();
    for ch in branch.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_') {
            slug.push(ch.to_ascii_lowercase());
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let slug: String = slug.trim_matches('-').chars().take(64).collect();
    if slug.is_empty() {
        format!("worktree-{:x}", branch.len())
    } else {
        slug
    }
}

fn git_output(cwd: &Path, args: &[&str]) -> Result<std::process::Output, Box<dyn Error>> {
    Ok(Command::new("git").arg("-C").arg(cwd).args(args).output()?)
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
    pub fn automation_record(&self, kind: Kind, id: u64) -> Result<Value, Box<dyn Error>> {
        match kind {
            Kind::Space => {
                let space = self
                    .spaces
                    .iter()
                    .find(|space| space.id == id)
                    .ok_or("Space does not exist")?;
                let kind = space.kind.map(SpaceKind::as_str).unwrap_or_else(|| {
                    if space
                        .tabs
                        .iter()
                        .flat_map(|tab| tab.layout.ids())
                        .find_map(|pane_id| {
                            self.panes
                                .iter()
                                .find(|pane| pane.id == pane_id)
                                .map(|pane| pane.launch.solmu())
                        })
                        .unwrap_or(false)
                    {
                        "solmu"
                    } else {
                        "terminal"
                    }
                });
                Ok(
                    json!({"id": space.id, "name": space.name, "cwd": space.directory, "kind": kind, "worktree": space.worktree.as_ref().map(|worktree| json!({"repo_root":worktree.repo_root,"branch":worktree.branch,"primary":worktree.primary})), "selected_tab": space.selected, "tabs": space.tabs.iter().map(|tab| tab.id).collect::<Vec<_>>() }),
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
                let native = parser
                    .callbacks()
                    .native
                    .as_ref()
                    .filter(|meta| meta.instance == pane.instance && pane.exited.is_none());
                Ok(
                    json!({"id": id, "space": space.id, "tab": tab.id, "name": pane.name, "cwd": pane.directory, "state": state, "thread": parser.callbacks().thread, "exited": pane.exited, "instance": pane.instance, "pid": pane.pid(), "controller": pane.terminal_lease, "launch": pane.launch, "rows": rows, "cols": cols, "native":native}),
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
            Request::TerminalOpen { .. } => {
                Err("Terminal streams belong to the session coordinator".into())
            }
            Request::WorktreeList { space } => {
                let source_id = space.unwrap_or(self.spaces[self.space].id);
                let source = self
                    .spaces
                    .iter()
                    .find(|space| space.id == source_id)
                    .ok_or("Source space does not exist")?;
                let output = git_output(&source.directory, &["worktree", "list", "--porcelain"])?;
                if !output.status.success() {
                    return Err(format!(
                        "git worktree list failed: {}",
                        String::from_utf8_lossy(&output.stderr).trim()
                    )
                    .into());
                }
                let text = String::from_utf8_lossy(&output.stdout);
                let mut worktrees = Vec::new();
                let mut path = None;
                let mut branch = None;
                let mut head = None;
                let mut detached = false;
                let mut append = |path: &mut Option<PathBuf>,
                                  branch: &mut Option<String>,
                                  head: &mut Option<String>,
                                  detached: &mut bool| {
                    if let Some(path) = path.take() {
                        let canonical_path = path.canonicalize().unwrap_or_else(|_| path.clone());
                        let open_space = self
                            .spaces
                            .iter()
                            .find(|space| space.directory == canonical_path)
                            .map(|space| space.id);
                        worktrees.push(json!({
                            "path": path,
                            "branch": branch.take(),
                            "head": head.take(),
                            "detached": *detached,
                            "primary": worktrees.is_empty(),
                            "open_space": open_space,
                        }));
                        *detached = false;
                    }
                };
                for line in text.lines() {
                    if line.is_empty() {
                        append(&mut path, &mut branch, &mut head, &mut detached);
                    } else if let Some(value) = line.strip_prefix("worktree ") {
                        path = Some(PathBuf::from(value));
                    } else if let Some(value) = line.strip_prefix("branch refs/heads/") {
                        branch = Some(value.to_owned());
                    } else if let Some(value) = line.strip_prefix("HEAD ") {
                        head = Some(value.to_owned());
                    } else if line == "detached" {
                        detached = true;
                    }
                }
                append(&mut path, &mut branch, &mut head, &mut detached);
                Ok(json!({"worktrees": worktrees}))
            }
            Request::AgentList => Ok(
                json!({"items":self.panes.iter().filter(|pane| pane.exited.is_none() && pane.launch.solmu()).map(|pane| self.automation_record(Kind::Pane, pane.id).unwrap()).collect::<Vec<_>>()}),
            ),
            Request::AgentGet { target } => {
                self.automation_record(Kind::Pane, crate::agent::resolve(self, target)?.id)
            }
            Request::AgentPrompt { .. }
            | Request::AgentWait { .. }
            | Request::AgentTurn { .. }
            | Request::AgentStop { .. } => {
                Err("Native requests belong to the session coordinator".into())
            }
            Request::AgentFocus { target, client } => {
                let id = crate::agent::resolve(self, target)?.id;
                self.automation_inner(&Request::Focus {
                    target: Kind::Pane,
                    id,
                    client: *client,
                })
            }
            Request::AgentRename {
                target,
                name: label,
            } => {
                let id = crate::agent::resolve(self, target)?.id;
                let label = name(label)?;
                if label.as_ref().is_some_and(|label| {
                    self.panes.iter().any(|pane| {
                        pane.id != id && pane.exited.is_none() && pane.name.as_ref() == Some(label)
                    })
                }) {
                    return Err("Another live Solmu pane already has that name".into());
                }
                self.automation_inner(&Request::Rename {
                    target: Kind::Pane,
                    id,
                    name: label,
                })
            }
            Request::AgentRead {
                target,
                lines,
                ansi,
            } => {
                let pane = crate::agent::resolve(self, target)?.id;
                self.automation_inner(&Request::Read {
                    pane,
                    lines: *lines,
                    ansi: *ansi,
                })
            }
            Request::AgentKeys { target, keys } => {
                let pane = crate::agent::resolve(self, target)?.id;
                self.automation_inner(&Request::SendKeys {
                    pane,
                    keys: keys.clone(),
                })
            }
            Request::Wait { .. } | Request::WaitOutput { .. } | Request::Subscribe { .. } => {
                Err("Monitor requests belong to the session coordinator".into())
            }
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
                cwd,
                name: label,
                launch,
                ..
            } => {
                let label = name(label)?;
                let cwd = directory(cwd.clone().map_or_else(|| self.new_cwd(), Ok)?)?;
                self.add_space_launch(cwd, launch.clone())?;
                self.spaces[self.space].name = label;
                self.automation_created()
            }
            Request::WorktreeCreate {
                space,
                branch,
                base,
                path,
                ..
            } => {
                if branch.trim() != branch
                    || branch.is_empty()
                    || branch.len() > 240
                    || branch.chars().any(char::is_control)
                {
                    return Err("Branch names must be 1 through 240 characters without surrounding whitespace or control characters".into());
                }
                if base.as_ref().is_some_and(|base| {
                    base.is_empty() || base.starts_with('-') || base.chars().any(char::is_control)
                }) {
                    return Err("Base must be a Git revision without control characters".into());
                }
                let source_id = space.unwrap_or(self.spaces[self.space].id);
                let source_index = self
                    .spaces
                    .iter()
                    .position(|space| space.id == source_id)
                    .ok_or("Source space does not exist")?;
                let source_directory = self.spaces[source_index].directory.clone();
                let repo_output = git_output(&source_directory, &["rev-parse", "--show-toplevel"])?;
                if !repo_output.status.success() {
                    return Err("The selected space is not inside a Git repository".into());
                }
                let repo_root = PathBuf::from(String::from_utf8_lossy(&repo_output.stdout).trim())
                    .canonicalize()?;
                let check = git_output(&repo_root, &["check-ref-format", "--branch", branch])?;
                if !check.status.success() {
                    return Err(format!(
                        "Invalid Git branch name: {}",
                        String::from_utf8_lossy(&check.stderr).trim()
                    )
                    .into());
                }
                let current = git_output(&repo_root, &["branch", "--show-current"])?;
                let parent_branch = String::from_utf8_lossy(&current.stdout).trim().to_owned();
                let target = if let Some(path) = path {
                    if !path.is_absolute() {
                        return Err("Worktree path must be absolute".into());
                    }
                    if path.exists() {
                        return Err("Worktree path already exists".into());
                    }
                    path.clone()
                } else {
                    let configured = PathBuf::from(&self.config.current.worktrees_directory);
                    let root = if let Some(rest) =
                        self.config.current.worktrees_directory.strip_prefix("~/")
                    {
                        let home =
                            std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
                                .ok_or(
                                    "Could not resolve the home directory for worktrees.directory",
                                )?;
                        PathBuf::from(home).join(rest)
                    } else if configured.is_absolute() {
                        configured
                    } else {
                        self.config.path.parent().unwrap().join(configured)
                    };
                    let repo_name = repo_root
                        .file_name()
                        .ok_or("Could not name the Git repository")?;
                    root.join(repo_name).join(worktree_slug(branch))
                };
                if target.exists() {
                    return Err(format!(
                        "Worktree destination already exists: {}",
                        target.display()
                    )
                    .into());
                }
                if let Some(parent) = target.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                let has_branch = git_output(
                    &repo_root,
                    &[
                        "show-ref",
                        "--verify",
                        "--quiet",
                        &format!("refs/heads/{branch}"),
                    ],
                )?
                .status
                .success();
                let mut add = Command::new("git");
                add.arg("-C").arg(&repo_root).args(["worktree", "add"]);
                if !has_branch {
                    add.arg("-b").arg(branch);
                }
                add.arg(&target);
                if has_branch {
                    add.arg(branch);
                } else {
                    add.arg(base.as_deref().unwrap_or("HEAD"));
                }
                let output = add.output()?;
                if !output.status.success() {
                    return Err(format!(
                        "git worktree add failed: {}",
                        String::from_utf8_lossy(&output.stderr).trim()
                    )
                    .into());
                }
                if let Err(error) = self.add_space(target.clone()) {
                    let _ = Command::new("git")
                        .arg("-C")
                        .arg(&repo_root)
                        .args(["worktree", "remove", "--force"])
                        .arg(&target)
                        .output();
                    return Err(error);
                }
                self.spaces[source_index].worktree = Some(super::WorktreeInfo {
                    repo_root: repo_root.clone(),
                    branch: parent_branch,
                    primary: true,
                });
                self.spaces[self.space].worktree = Some(super::WorktreeInfo {
                    repo_root,
                    branch: branch.clone(),
                    primary: false,
                });
                let created = self.automation_created()?;
                Ok(
                    json!({"created": created, "worktree": {"path": target, "branch": branch, "base": if has_branch { Value::Null } else { json!(base.as_deref().unwrap_or("HEAD")) }}}),
                )
            }
            Request::CreateTab {
                space,
                cwd,
                name: label,
                launch,
                ..
            } => {
                let label = name(label)?;
                self.automation_focus(Kind::Space, *space)?;
                let cwd = directory(cwd.clone().map_or_else(|| self.new_cwd(), Ok)?)?;
                self.add_tab_launch(cwd, launch.clone())?;
                self.tab_mut().name = label;
                self.automation_created()
            }
            Request::SplitPane {
                pane,
                axis,
                ratio: value,
                cwd,
                name: label,
                launch,
                ..
            } => {
                let label = name(label)?;
                ratio(*value)?;
                self.automation_focus(Kind::Pane, *pane)?;
                let cwd = directory(cwd.clone().map_or_else(|| self.new_cwd(), Ok)?)?;
                self.split_launch(*axis, cwd, *value, launch.clone())?;
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
                let mut fresh = Pane::start(
                    old.id,
                    directory(old.directory.clone())?,
                    &self.executable,
                    old.launch.clone(),
                )?;
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
