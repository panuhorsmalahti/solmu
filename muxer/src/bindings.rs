use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    NewTab,
    NewSpace,
    NewWorktree,
    PreviousSpace,
    NextSpace,
    NextTab,
    PreviousTab,
    SelectTab(u8),
    SplitRight,
    SplitDown,
    ShellPane,
    RunCommand,
    FocusLeft,
    FocusDown,
    FocusUp,
    FocusRight,
    SwapLeft,
    SwapDown,
    SwapUp,
    SwapRight,
    Zoom,
    ClosePane,
    CloseTab,
    CloseSpace,
    RenameSpace,
    RenameTab,
    RenamePane,
    Find,
    Navigate,
    Help,
    Settings,
    ResizeOrRestart,
    Restart,
    Detach,
    SendPrefix,
    ToggleSidebar,
}
pub struct Definition {
    pub action: Action,
    pub name: String,
    pub description: String,
    pub defaults: Vec<String>,
}
pub fn definitions() -> Vec<Definition> {
    use Action::*;
    let mut result: Vec<_> = [
        (NewTab, "new_tab", "New tab", &["prefix+n", "prefix+c"][..]),
        (NewSpace, "new_space", "New space", &["prefix+w"]),
        (
            NewWorktree,
            "new_worktree",
            "New Git worktree",
            &["prefix+shift+g"],
        ),
        (
            PreviousSpace,
            "previous_space",
            "Previous space",
            &["prefix+up"],
        ),
        (NextSpace, "next_space", "Next space", &["prefix+down"]),
        (NextTab, "next_tab", "Next tab", &["prefix+tab", "prefix+]"]),
        (
            PreviousTab,
            "previous_tab",
            "Previous tab",
            &["prefix+backtab", "prefix+[", "prefix+p"],
        ),
        (
            SplitRight,
            "split_right",
            "Split right",
            &["prefix+s", "prefix+v"],
        ),
        (SplitDown, "split_down", "Split down", &["prefix+minus"]),
        (ShellPane, "shell_pane", "Shell pane", &["prefix+t"]),
        (RunCommand, "run_command", "Run command", &["prefix+!"]),
        (
            FocusLeft,
            "focus_left",
            "Focus pane left",
            &["prefix+h", "prefix+left"],
        ),
        (FocusDown, "focus_down", "Focus pane down", &["prefix+j"]),
        (FocusUp, "focus_up", "Focus pane up", &["prefix+k"]),
        (
            FocusRight,
            "focus_right",
            "Focus pane right",
            &["prefix+l", "prefix+right"],
        ),
        (
            SwapLeft,
            "swap_left",
            "Swap pane left",
            &["prefix+shift+h", "prefix+shift+left"],
        ),
        (SwapDown, "swap_down", "Swap pane down", &["prefix+shift+j"]),
        (SwapUp, "swap_up", "Swap pane up", &["prefix+shift+k"]),
        (
            SwapRight,
            "swap_right",
            "Swap pane right",
            &["prefix+shift+l", "prefix+shift+right"],
        ),
        (Zoom, "zoom", "Zoom / restore pane", &["prefix+z"]),
        (ClosePane, "close_pane", "Close pane", &["prefix+x"]),
        (CloseTab, "close_tab", "Close tab", &["prefix+shift+x"]),
        (
            CloseSpace,
            "close_space",
            "Close space",
            &["prefix+shift+d"],
        ),
        (
            RenameSpace,
            "rename_space",
            "Rename space",
            &["prefix+shift+w"],
        ),
        (RenameTab, "rename_tab", "Rename tab", &["prefix+shift+t"]),
        (
            RenamePane,
            "rename_pane",
            "Rename pane",
            &["prefix+shift+p"],
        ),
        (Find, "find", "Find spaces, tabs and panes", &["prefix+g"]),
        (Navigate, "navigate", "Navigation mode", &["prefix+m"]),
        (Help, "help", "Search keyboard help", &["prefix+?"]),
        (Settings, "settings", "Settings", &["prefix+comma"]),
        (
            ResizeOrRestart,
            "resize_or_restart",
            "Resize running pane / restart exited pane",
            &["prefix+r"],
        ),
        (Restart, "restart", "Restart exited pane", &[]),
        (
            Detach,
            "detach",
            "Detach / quit foreground session",
            &["prefix+q"],
        ),
        (
            SendPrefix,
            "send_prefix",
            "Send literal prefix key",
            &["prefix+b"],
        ),
        (
            ToggleSidebar,
            "toggle_sidebar",
            "Show / hide sidebar",
            &["prefix+shift+b"],
        ),
    ]
    .into_iter()
    .map(|(action, name, description, defaults)| Definition {
        action,
        name: name.into(),
        description: description.into(),
        defaults: defaults.iter().map(|key| (*key).into()).collect(),
    })
    .collect();
    for index in 1..=8 {
        result.push(Definition {
            action: SelectTab(index - 1),
            name: format!("select_tab_{index}"),
            description: format!("Select tab {index}"),
            defaults: vec![format!("prefix+{index}")],
        });
    }
    result
}

#[derive(Clone, PartialEq, Eq)]
pub struct Chord {
    pub prefixed: bool,
    code: KeyCode,
    modifiers: KeyModifiers,
}
fn normalize(code: KeyCode, mut modifiers: KeyModifiers) -> (KeyCode, KeyModifiers) {
    let code = match code {
        KeyCode::Char(ch) if ch.is_ascii_uppercase() => {
            modifiers |= KeyModifiers::SHIFT;
            KeyCode::Char(ch.to_ascii_lowercase())
        }
        KeyCode::BackTab => {
            modifiers |= KeyModifiers::SHIFT;
            KeyCode::Tab
        }
        // ASCII punctuation already carries its shifted value. Older terminals
        // omit Shift while Windows and modern keyboard protocols retain it.
        KeyCode::Char(ch) if !ch.is_ascii_alphabetic() => {
            modifiers.remove(KeyModifiers::SHIFT);
            KeyCode::Char(ch)
        }
        code => code,
    };
    (code, modifiers)
}
impl Chord {
    pub fn parse(text: &str, allow_prefix: bool) -> Result<Self, String> {
        let pieces: Vec<_> = text.split('+').collect();
        let (last, modifiers) = pieces.split_last().ok_or("Empty shortcut")?;
        let mut prefixed = false;
        let mut mods = KeyModifiers::NONE;
        for modifier in modifiers {
            match modifier.to_ascii_lowercase().as_str() {
                "prefix" if allow_prefix && !prefixed => prefixed = true,
                "ctrl" | "control" if !mods.contains(KeyModifiers::CONTROL) => {
                    mods |= KeyModifiers::CONTROL
                }
                "alt" | "meta" if !mods.contains(KeyModifiers::ALT) => mods |= KeyModifiers::ALT,
                "shift" if !mods.contains(KeyModifiers::SHIFT) => mods |= KeyModifiers::SHIFT,
                "super" if !mods.contains(KeyModifiers::SUPER) => mods |= KeyModifiers::SUPER,
                _ => return Err(format!("Unknown or repeated shortcut modifier in {text:?}")),
            }
        }
        let code = match last.to_ascii_lowercase().as_str() {
            "up" => KeyCode::Up,
            "down" => KeyCode::Down,
            "left" => KeyCode::Left,
            "right" => KeyCode::Right,
            "home" => KeyCode::Home,
            "end" => KeyCode::End,
            "pageup" => KeyCode::PageUp,
            "pagedown" => KeyCode::PageDown,
            "enter" | "return" => KeyCode::Enter,
            "esc" | "escape" => KeyCode::Esc,
            "tab" => KeyCode::Tab,
            "backtab" => KeyCode::BackTab,
            "backspace" => KeyCode::Backspace,
            "delete" => KeyCode::Delete,
            "insert" => KeyCode::Insert,
            "space" => KeyCode::Char(' '),
            "plus" => KeyCode::Char('+'),
            "minus" => KeyCode::Char('-'),
            "comma" => KeyCode::Char(','),
            "slash" => KeyCode::Char('/'),
            "backslash" => KeyCode::Char('\\'),
            value
                if value.starts_with('f')
                    && value[1..]
                        .parse::<u8>()
                        .is_ok_and(|n| (1..=24).contains(&n)) =>
            {
                KeyCode::F(value[1..].parse().unwrap())
            }
            _ if last.chars().count() == 1 && !last.chars().next().unwrap().is_control() => {
                KeyCode::Char(last.chars().next().unwrap())
            }
            _ => return Err(format!("Unknown shortcut key in {text:?}")),
        };
        let (code, modifiers) = normalize(code, mods);
        Ok(Self {
            prefixed,
            code,
            modifiers,
        })
    }
    pub fn matches(&self, key: KeyEvent) -> bool {
        (self.code, self.modifiers) == normalize(key.code, key.modifiers)
    }
    pub fn event(&self) -> KeyEvent {
        KeyEvent::new(self.code, self.modifiers)
    }
}
pub struct Binding {
    pub action: Action,
    pub chord: Chord,
}
pub struct Keymap {
    pub prefix: Chord,
    pub prefix_text: String,
    pub bindings: Vec<Binding>,
    pub specs: BTreeMap<String, Vec<String>>,
}
impl Keymap {
    pub fn new(prefix: String, overrides: BTreeMap<String, Vec<String>>) -> Result<Self, String> {
        let prefix_chord = Chord::parse(&prefix, false)?;
        if prefix_chord.modifiers.is_empty()
            && matches!(
                prefix_chord.code,
                KeyCode::Char(_) | KeyCode::Enter | KeyCode::Tab | KeyCode::Backspace
            )
        {
            return Err(
                "Use a modified key, Escape, or a function/navigation key for keys.prefix".into(),
            );
        }
        let definitions = definitions();
        if let Some(name) = overrides.keys().find(|name| {
            !definitions
                .iter()
                .any(|definition| &definition.name == *name)
        }) {
            return Err(format!("Unknown action keys.{name}"));
        }
        let mut bindings: Vec<Binding> = vec![];
        let mut specs = BTreeMap::new();
        for definition in definitions {
            let keys = overrides
                .get(&definition.name)
                .unwrap_or(&definition.defaults);
            for key in keys {
                let chord = Chord::parse(key, true)?;
                if !chord.prefixed && chord == prefix_chord {
                    return Err(format!(
                        "keys.{} conflicts with the prefix key",
                        definition.name
                    ));
                }
                if let Some(previous) = bindings.iter().find(|binding| binding.chord == chord) {
                    if previous.action == definition.action {
                        continue;
                    }
                    return Err(format!(
                        "Duplicate shortcut {key:?}; change both actions or unbind one with []"
                    ));
                }
                bindings.push(Binding {
                    action: definition.action,
                    chord,
                });
            }
            specs.insert(definition.name, keys.clone());
        }
        Ok(Self {
            prefix: prefix_chord,
            prefix_text: prefix,
            bindings,
            specs,
        })
    }
    pub fn resolve(&self, key: KeyEvent, prefixed: bool) -> Option<Action> {
        self.bindings
            .iter()
            .find(|binding| binding.chord.prefixed == prefixed && binding.chord.matches(key))
            .map(|binding| binding.action)
    }
    pub fn help(&self) -> Vec<(String, String)> {
        definitions()
            .into_iter()
            .map(|definition| {
                let keys = &self.specs[&definition.name];
                let shortcuts = if keys.is_empty() {
                    "Unbound".into()
                } else {
                    keys.iter()
                        .map(|key| key.replace("prefix+", &format!("{} then ", self.prefix_text)))
                        .collect::<Vec<_>>()
                        .join(" / ")
                };
                (shortcuts, definition.description)
            })
            .collect()
    }
}
