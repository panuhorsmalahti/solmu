use crate::{
    bindings::{self, Keymap},
    session,
};
use ratatui::{layout::Rect, style::Color};
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use toml_edit::{DocumentMut, Item, TableLike, value};

pub const DEFAULT: &str = r##"# Solmu Muxer settings. Changes apply automatically to running sessions.
[keys]
prefix = "ctrl+b"
# Each action accepts one shortcut, a list of shortcuts, or [] to unbind it.
# new_tab = ["prefix+n", "prefix+c", "ctrl+alt+n"]
# find = "prefix+g"

[theme]
# solmu, light, or terminal (your terminal's ANSI palette).
name = "solmu"

[theme.colors]
# Override background, panel, selected, text, muted, or accent.
# accent = "#9ccfb0"

[ui]
sidebar_visible = true
sidebar_width = 26

[server]
headless_columns = 120
headless_rows = 40

[workspace]
# New panes/tabs: follow the selected pane, current server directory, home, or path.
new_cwd = "follow"
# path = "/path/to/project"

[terminal]
# New panes default to Solmu. Choose shell to start an interactive shell instead.
new_pane = "solmu"
# Executable name/path; empty chooses SHELL or /bin/sh on Unix, PowerShell on Windows.
shell = ""
# auto uses login shells on macOS; login or non_login override it on Unix.
shell_mode = "auto"
"##;
pub fn default_text() -> String {
    let keys: String = bindings::definitions()
        .into_iter()
        .map(|definition| {
            let mut array = toml_edit::Array::new();
            for key in definition.defaults {
                array.push(key);
            }
            format!(
                "# {}\n# {} = {array}\n",
                definition.description, definition.name
            )
        })
        .collect();
    DEFAULT.replace(
        "# new_tab = [\"prefix+n\", \"prefix+c\", \"ctrl+alt+n\"]\n# find = \"prefix+g\"",
        &keys,
    )
}

#[derive(Clone, Copy)]
pub struct Palette {
    pub background: Color,
    pub panel: Color,
    pub selected: Color,
    pub text: Color,
    pub muted: Color,
    pub accent: Color,
}
pub struct Config {
    pub keys: Keymap,
    pub theme: String,
    pub colors: BTreeMap<String, String>,
    pub sidebar_visible: bool,
    pub sidebar_width: u16,
    pub columns: u16,
    pub rows: u16,
    pub cwd_policy: String,
    pub cwd_path: String,
    pub new_pane: String,
    pub shell: String,
    pub shell_mode: String,
}
fn section<'a>(doc: &'a DocumentMut, name: &str) -> Result<Option<&'a dyn TableLike>, String> {
    doc.get(name)
        .map(|item| {
            item.as_table_like()
                .ok_or_else(|| format!("{name} must be a table"))
        })
        .transpose()
}
fn get<'a>(section: Option<&'a dyn TableLike>, name: &str) -> Option<&'a Item> {
    section.and_then(|table| table.get(name))
}
fn string(item: Option<&Item>, default: &str, name: &str) -> Result<String, String> {
    item.map_or_else(
        || Ok(default.into()),
        |item| {
            item.as_str()
                .map(str::to_owned)
                .ok_or_else(|| format!("{name} must be a string"))
        },
    )
}
fn integer(
    item: Option<&Item>,
    default: u16,
    min: i64,
    max: i64,
    name: &str,
) -> Result<u16, String> {
    match item {
        None => Ok(default),
        Some(item) => item
            .as_integer()
            .filter(|value| (min..=max).contains(value))
            .map(|value| value as u16)
            .ok_or_else(|| format!("{name} must be an integer from {min} to {max}")),
    }
}
fn boolean(item: Option<&Item>, default: bool, name: &str) -> Result<bool, String> {
    item.map_or(Ok(default), |item| {
        item.as_bool()
            .ok_or_else(|| format!("{name} must be true or false"))
    })
}
fn known(table: Option<&dyn TableLike>, names: &[&str], section: &str) -> Result<(), String> {
    if let Some(table) = table {
        for (name, _) in table.iter() {
            if !names.contains(&name) {
                return Err(format!("Unknown setting {section}{name}"));
            }
        }
    }
    Ok(())
}
pub fn color(text: &str) -> Result<Color, String> {
    let lower = text.to_ascii_lowercase();
    match lower.as_str() {
        "reset" | "default" | "transparent" => Ok(Color::Reset),
        "black" => Ok(Color::Black),
        "white" => Ok(Color::White),
        "red" => Ok(Color::Red),
        "green" => Ok(Color::Green),
        "blue" => Ok(Color::Blue),
        "yellow" => Ok(Color::Yellow),
        "cyan" => Ok(Color::Cyan),
        "magenta" => Ok(Color::Magenta),
        "gray" | "grey" => Ok(Color::Gray),
        "darkgray" => Ok(Color::DarkGray),
        _ if text.len() == 7 && text.starts_with('#') => u32::from_str_radix(&text[1..], 16)
            .map(|value| Color::Rgb((value >> 16) as u8, (value >> 8) as u8, value as u8))
            .map_err(|_| format!("Invalid color {text:?}")),
        _ => Err(format!(
            "Invalid color {text:?}; use #RRGGBB, a named ANSI color, or reset"
        )),
    }
}
impl Config {
    pub fn parse(text: &str) -> Result<Self, String> {
        let text = text.strip_prefix('\u{feff}').unwrap_or(text);
        let doc = text
            .parse::<DocumentMut>()
            .map_err(|error| error.to_string())?;
        known(
            Some(doc.as_table()),
            &["keys", "theme", "ui", "server", "workspace", "terminal"],
            "",
        )?;
        let keys = section(&doc, "keys")?;
        let mut overrides = BTreeMap::new();
        if let Some(keys) = keys {
            for (name, item) in keys.iter().filter(|(name, _)| *name != "prefix") {
                let spec = if let Some(text) = item.as_str() {
                    vec![text.into()]
                } else if let Some(array) = item.as_array() {
                    array
                        .iter()
                        .map(|value| {
                            value
                                .as_str()
                                .map(str::to_owned)
                                .ok_or_else(|| format!("keys.{name} must contain strings"))
                        })
                        .collect::<Result<Vec<_>, _>>()?
                } else {
                    return Err(format!("keys.{name} must be a shortcut string or array"));
                };
                if spec.len() > 16 {
                    return Err(format!("keys.{name} accepts at most 16 shortcuts"));
                }
                overrides.insert(name.into(), spec);
            }
        }
        let keys = Keymap::new(
            string(get(keys, "prefix"), "ctrl+b", "keys.prefix")?,
            overrides,
        )?;
        let theme = section(&doc, "theme")?;
        known(theme, &["name", "colors"], "theme.")?;
        let theme_name = string(get(theme, "name"), "solmu", "theme.name")?;
        if !["solmu", "light", "terminal"].contains(&theme_name.as_str()) {
            return Err("theme.name must be solmu, light, or terminal".into());
        }
        let mut colors = BTreeMap::new();
        if let Some(item) = get(theme, "colors") {
            let table = item.as_table_like().ok_or("theme.colors must be a table")?;
            known(
                Some(table),
                &["background", "panel", "selected", "text", "muted", "accent"],
                "theme.colors.",
            )?;
            for (name, item) in table.iter() {
                let text = string(Some(item), "", &format!("theme.colors.{name}"))?;
                color(&text)?;
                colors.insert(name.into(), text);
            }
        }
        let ui = section(&doc, "ui")?;
        known(ui, &["sidebar_visible", "sidebar_width"], "ui.")?;
        let server = section(&doc, "server")?;
        known(server, &["headless_columns", "headless_rows"], "server.")?;
        let workspace = section(&doc, "workspace")?;
        known(workspace, &["new_cwd", "path"], "workspace.")?;
        let cwd_policy = string(get(workspace, "new_cwd"), "follow", "workspace.new_cwd")?;
        if !["follow", "current", "home", "path"].contains(&cwd_policy.as_str()) {
            return Err("workspace.new_cwd must be follow, current, home, or path".into());
        }
        let cwd_path = string(get(workspace, "path"), "", "workspace.path")?;
        if cwd_policy == "path" && cwd_path.trim().is_empty() {
            return Err("workspace.path is required when workspace.new_cwd is path".into());
        }
        let terminal = section(&doc, "terminal")?;
        known(terminal, &["new_pane", "shell", "shell_mode"], "terminal.")?;
        let new_pane = string(get(terminal, "new_pane"), "solmu", "terminal.new_pane")?;
        if !["solmu", "shell"].contains(&new_pane.as_str()) {
            return Err("terminal.new_pane must be solmu or shell".into());
        }
        let shell = string(get(terminal, "shell"), "", "terminal.shell")?;
        if shell.contains('\0') || shell.len() > 32768 {
            return Err("terminal.shell must be an executable name or path".into());
        }
        let shell_mode = string(get(terminal, "shell_mode"), "auto", "terminal.shell_mode")?;
        if !["auto", "login", "non_login"].contains(&shell_mode.as_str()) {
            return Err("terminal.shell_mode must be auto, login, or non_login".into());
        }
        Ok(Self {
            keys,
            theme: theme_name,
            colors,
            sidebar_visible: boolean(get(ui, "sidebar_visible"), true, "ui.sidebar_visible")?,
            sidebar_width: integer(get(ui, "sidebar_width"), 26, 10, 80, "ui.sidebar_width")?,
            columns: integer(
                get(server, "headless_columns"),
                120,
                20,
                240,
                "server.headless_columns",
            )?,
            rows: integer(
                get(server, "headless_rows"),
                40,
                8,
                100,
                "server.headless_rows",
            )?,
            cwd_policy,
            cwd_path,
            new_pane,
            shell,
            shell_mode,
        })
    }
    pub fn launch(
        &self,
        requested: Option<crate::launch::Launch>,
    ) -> Result<crate::launch::Launch, String> {
        use crate::launch::Launch;
        let launch = requested.unwrap_or_else(|| {
            if self.new_pane == "shell" {
                Launch::Shell { argv: vec![] }
            } else {
                Launch::Solmu
            }
        });
        let launch = match launch {
            Launch::Shell { argv } if argv.is_empty() => {
                let program = if self.shell.trim().is_empty() {
                    if cfg!(windows) {
                        "powershell.exe".into()
                    } else {
                        std::env::var("SHELL")
                            .ok()
                            .filter(|v| !v.trim().is_empty())
                            .unwrap_or_else(|| "/bin/sh".into())
                    }
                } else {
                    self.shell.clone()
                };
                let mut argv = vec![program];
                if cfg!(windows) && self.shell.trim().is_empty() {
                    argv.push("-NoLogo".into());
                } else if cfg!(unix)
                    && (self.shell_mode == "login"
                        || (self.shell_mode == "auto" && cfg!(target_os = "macos")))
                {
                    argv.push("-l".into());
                }
                Launch::Shell { argv }
            }
            other => other,
        };
        launch.validate()?;
        Ok(launch)
    }
    pub fn palette(&self) -> Palette {
        let mut palette = match self.theme.as_str() {
            "light" => Palette {
                background: Color::Rgb(247, 248, 243),
                panel: Color::Rgb(232, 238, 228),
                selected: Color::Rgb(213, 226, 209),
                text: Color::Rgb(35, 48, 42),
                muted: Color::Rgb(93, 112, 98),
                accent: Color::Rgb(38, 108, 66),
            },
            "terminal" => Palette {
                background: Color::Reset,
                panel: Color::Reset,
                selected: Color::DarkGray,
                text: Color::Reset,
                muted: Color::DarkGray,
                accent: Color::Green,
            },
            _ => Palette {
                background: Color::Rgb(17, 27, 29),
                panel: Color::Rgb(29, 48, 49),
                selected: Color::Rgb(48, 84, 73),
                text: Color::Rgb(231, 238, 232),
                muted: Color::Rgb(159, 180, 170),
                accent: Color::Rgb(169, 223, 185),
            },
        };
        for (name, text) in &self.colors {
            let target = match name.as_str() {
                "background" => &mut palette.background,
                "panel" => &mut palette.panel,
                "selected" => &mut palette.selected,
                "text" => &mut palette.text,
                "muted" => &mut palette.muted,
                _ => &mut palette.accent,
            };
            *target = color(text).expect("validated color");
        }
        palette
    }
    pub fn headless_area(&self) -> Rect {
        Rect::new(0, 0, self.columns, self.rows)
    }
    pub fn cwd(&self, follow: &Path, base: &Path) -> Result<PathBuf, String> {
        let path = match self.cwd_policy.as_str() {
            "home" => PathBuf::from(
                std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
                    .ok_or("Home directory is unavailable")?,
            ),
            "current" => std::env::current_dir().map_err(|error| error.to_string())?,
            "path" => {
                let path = PathBuf::from(&self.cwd_path);
                if path.is_absolute() {
                    path
                } else {
                    base.join(path)
                }
            }
            _ => follow.into(),
        };
        let path = path
            .canonicalize()
            .map_err(|error| format!("Workspace unavailable: {error}"))?;
        if !path.is_dir() {
            return Err("Workspace must be a directory".into());
        }
        Ok(path)
    }
    pub fn settings(&self) -> Vec<(String, String)> {
        let mut result = vec![
            ("theme.name".into(), self.theme.clone()),
            (
                "ui.sidebar_visible".into(),
                self.sidebar_visible.to_string(),
            ),
            ("ui.sidebar_width".into(), self.sidebar_width.to_string()),
            ("server.headless_columns".into(), self.columns.to_string()),
            ("server.headless_rows".into(), self.rows.to_string()),
            ("workspace.new_cwd".into(), self.cwd_policy.clone()),
            ("workspace.path".into(), self.cwd_path.clone()),
            ("terminal.new_pane".into(), self.new_pane.clone()),
            ("terminal.shell".into(), self.shell.clone()),
            ("terminal.shell_mode".into(), self.shell_mode.clone()),
            ("keys.prefix".into(), self.keys.prefix_text.clone()),
        ];
        result.extend(
            ["background", "panel", "selected", "text", "muted", "accent"].map(|name| {
                (
                    format!("theme.colors.{name}"),
                    self.colors.get(name).cloned().unwrap_or_default(),
                )
            }),
        );
        result.extend(bindings::definitions().into_iter().map(|definition| {
            (
                format!("keys.{}", definition.name),
                self.keys.specs[&definition.name].join(", "),
            )
        }));
        result
    }
    pub fn hint(name: &str) -> &'static str {
        match name {
            "theme.name" => "solmu, light, or terminal",
            "ui.sidebar_visible" => "true or false",
            "ui.sidebar_width" => "10–80 columns",
            "server.headless_columns" => "20–240 columns",
            "server.headless_rows" => "8–100 rows",
            "workspace.new_cwd" => "follow, current, home, or path",
            "workspace.path" => "directory; relative to this config file",
            "terminal.new_pane" => "solmu or shell; applies to new panes only",
            "terminal.shell" => "executable name or path; empty uses platform default",
            "terminal.shell_mode" => "auto, login, or non_login",
            "keys.prefix" => "e.g. ctrl+b, ctrl+a, f12",
            name if name.starts_with("keys.") => "comma-separated shortcuts; empty unbinds",
            _ => "#RRGGBB, named color, reset; empty uses theme default",
        }
    }
}
pub struct Manager {
    pub path: PathBuf,
    pub current: Config,
    pub error: Option<String>,
    pub generation: u64,
    observed: Option<Vec<u8>>,
    checked: Instant,
}
fn read(path: &Path) -> Result<Option<Vec<u8>>, String> {
    match fs::File::open(path) {
        Ok(file) => {
            let mut bytes = Vec::new();
            file.take(65537)
                .read_to_end(&mut bytes)
                .map_err(|error| error.to_string())?;
            if bytes.len() > 65536 {
                return Err("Configuration exceeds 64 KiB".into());
            }
            Ok(Some(bytes))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.to_string()),
    }
}
impl Manager {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let path = match std::env::var_os("SOLMU_MUXER_CONFIG") {
            Some(path) => {
                let path = PathBuf::from(path);
                if path.is_absolute() {
                    path
                } else {
                    std::env::current_dir()?.join(path)
                }
            }
            None => session::root()?.join("config.toml"),
        };
        let mut manager = Self {
            path,
            current: Config::parse("").expect("valid defaults"),
            error: None,
            generation: 0,
            observed: None,
            checked: Instant::now(),
        };
        manager.refresh(true);
        Ok(manager)
    }
    pub fn refresh(&mut self, force: bool) -> bool {
        if !force && self.checked.elapsed() < Duration::from_millis(400) {
            return false;
        }
        self.checked = Instant::now();
        let contents = match read(&self.path) {
            Ok(contents) => contents,
            Err(error) => {
                self.error = Some(error);
                return false;
            }
        };
        if !force && contents == self.observed && self.error.is_none() {
            return false;
        }
        let result = std::str::from_utf8(contents.as_deref().unwrap_or_default())
            .map_err(|_| "Configuration must be UTF-8".to_owned())
            .and_then(Config::parse);
        self.observed = contents;
        match result {
            Ok(config) => {
                self.current = config;
                self.error = None;
                self.generation += 1;
                true
            }
            Err(error) => {
                self.error = Some(error);
                false
            }
        }
    }
    pub fn set(&mut self, name: &str, text: &str, original: &str) -> Result<(), String> {
        let parent = self
            .path
            .parent()
            .ok_or("Configuration path has no parent")?;
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        let lock_path = parent.join(format!(
            ".{}.lock",
            self.path
                .file_name()
                .ok_or("Configuration path has no file name")?
                .to_string_lossy()
        ));
        let lock = session::private_file(&lock_path, false).map_err(|error| error.to_string())?;
        lock.try_lock()
            .map_err(|_| "Another terminal is saving settings. Try again.")?;
        let contents = read(&self.path)?;
        let bytes = contents.as_deref().unwrap_or_default();
        let source = std::str::from_utf8(bytes).map_err(|_| "Configuration must be UTF-8")?;
        let config = Config::parse(source)
            .map_err(|_| "Fix the configuration file before saving a setting")?;
        let current = config
            .settings()
            .into_iter()
            .find(|(key, _)| key == name)
            .ok_or("Unknown setting")?
            .1;
        if current != original {
            return Err(
                "This setting changed in another terminal. Cancel and open it again.".into(),
            );
        }
        let mut doc = source
            .strip_prefix('\u{feff}')
            .unwrap_or(source)
            .parse::<DocumentMut>()
            .map_err(|error| error.to_string())?;
        let pieces: Vec<_> = name.split('.').collect();
        let mut item = if name == "ui.sidebar_visible" {
            value(text.parse::<bool>().map_err(|_| "Enter true or false")?)
        } else if name == "ui.sidebar_width" || name.starts_with("server.") {
            value(text.parse::<i64>().map_err(|_| "Enter an integer")?)
        } else if name.starts_with("keys.") && name != "keys.prefix" {
            let mut array = toml_edit::Array::new();
            for shortcut in text
                .split(',')
                .map(str::trim)
                .filter(|text| !text.is_empty())
            {
                array.push(shortcut);
            }
            value(array)
        } else if pieces.len() == 3 && text.is_empty() {
            Item::None
        } else {
            value(text)
        };
        let previous = doc
            .get(pieces[0])
            .and_then(Item::as_table_like)
            .and_then(|table| table.get(pieces[1]));
        let previous = if pieces.len() == 3 {
            previous
                .and_then(Item::as_table_like)
                .and_then(|table| table.get(pieces[2]))
        } else {
            previous
        };
        if let Some(decor) = previous
            .and_then(Item::as_value)
            .map(|value| value.decor().clone())
            && let Some(value) = item.as_value_mut()
        {
            *value.decor_mut() = decor;
        }
        if pieces.len() == 3 {
            doc[pieces[0]][pieces[1]][pieces[2]] = item;
        } else {
            doc[pieces[0]][pieces[1]] = item;
        }
        let source = if source.starts_with('\u{feff}') {
            format!("\u{feff}{doc}")
        } else {
            doc.to_string()
        };
        Config::parse(&source)?;
        // Merge unrelated edits from the latest valid file, preserving comments.
        // Avoid replacing a file that changed during validation.
        if read(&self.path)? != contents {
            return Err("Configuration changed while saving. Try again.".into());
        }
        let parent = self
            .path
            .parent()
            .ok_or("Configuration path has no parent")?;
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        let temporary = parent.join(format!(".muxer-config-{}.tmp", uuid::Uuid::new_v4()));
        let result = (|| -> Result<(), String> {
            let mut file =
                session::private_file(&temporary, false).map_err(|error| error.to_string())?;
            file.write_all(source.as_bytes())
                .map_err(|error| error.to_string())?;
            file.sync_all().map_err(|error| error.to_string())?;
            drop(file);
            #[cfg(windows)]
            crate::windows::replace(&temporary, &self.path).map_err(|error| error.to_string())?;
            #[cfg(not(windows))]
            fs::rename(&temporary, &self.path).map_err(|error| error.to_string())?;
            Ok(())
        })();
        let _ = fs::remove_file(temporary);
        result?;
        self.refresh(true);
        Ok(())
    }
}
