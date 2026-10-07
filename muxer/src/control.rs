//! Typed local automation requests, shared by the CLI and authenticated transport.
use crate::{bindings::Chord, keys::encode, layout::Axis, session};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{ffi::OsString, path::PathBuf};

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Space,
    Tab,
    Pane,
}
#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Zoom {
    On,
    Off,
    Toggle,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum AgentTarget {
    Id(u64),
    Name(String),
}
#[derive(Deserialize, Serialize)]
#[serde(
    tag = "method",
    content = "params",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum Request {
    TerminalOpen {
        target: AgentTarget,
        #[serde(default)]
        observe: bool,
        #[serde(default)]
        takeover: bool,
        #[serde(default)]
        cols: Option<u16>,
        #[serde(default)]
        rows: Option<u16>,
    },
    AgentList,
    AgentGet {
        target: AgentTarget,
    },
    AgentPrompt {
        target: AgentTarget,
        text: String,
        #[serde(default)]
        wait: bool,
        #[serde(default)]
        timeout_ms: Option<u64>,
    },
    AgentWait {
        target: AgentTarget,
        #[serde(default)]
        turn: Option<String>,
        #[serde(default)]
        timeout_ms: Option<u64>,
    },
    AgentTurn {
        target: AgentTarget,
        turn: String,
    },
    AgentStop {
        target: AgentTarget,
    },
    AgentRename {
        target: AgentTarget,
        name: Option<String>,
    },
    AgentFocus {
        target: AgentTarget,
        #[serde(default)]
        client: Option<u64>,
    },
    AgentRead {
        target: AgentTarget,
        #[serde(default)]
        lines: Option<u16>,
        #[serde(default)]
        ansi: bool,
    },
    AgentKeys {
        target: AgentTarget,
        keys: Vec<String>,
    },
    Snapshot,
    List {
        target: Kind,
        #[serde(default)]
        parent: Option<u64>,
    },
    Get {
        target: Kind,
        id: u64,
    },
    CreateSpace {
        #[serde(default)]
        launch: Option<crate::launch::Launch>,
        #[serde(default)]
        cwd: Option<PathBuf>,
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        focus: bool,
    },
    WorktreeCreate {
        #[serde(default)]
        space: Option<u64>,
        branch: String,
        #[serde(default)]
        base: Option<String>,
        #[serde(default)]
        path: Option<PathBuf>,
        #[serde(default)]
        focus: bool,
    },
    WorktreeList {
        #[serde(default)]
        space: Option<u64>,
    },
    WorktreeOpen {
        #[serde(default)]
        space: Option<u64>,
        #[serde(default)]
        branch: Option<String>,
        #[serde(default)]
        path: Option<PathBuf>,
        #[serde(default)]
        focus: bool,
    },
    CreateTab {
        #[serde(default)]
        launch: Option<crate::launch::Launch>,
        space: u64,
        #[serde(default)]
        cwd: Option<PathBuf>,
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        focus: bool,
    },
    SplitPane {
        #[serde(default)]
        launch: Option<crate::launch::Launch>,
        pane: u64,
        axis: Axis,
        #[serde(default = "half")]
        ratio: u16,
        #[serde(default)]
        cwd: Option<PathBuf>,
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        focus: bool,
    },
    Focus {
        target: Kind,
        id: u64,
        #[serde(default)]
        client: Option<u64>,
    },
    Rename {
        target: Kind,
        id: u64,
        name: Option<String>,
    },
    Close {
        target: Kind,
        id: u64,
    },
    Swap {
        pane: u64,
        other: u64,
    },
    Zoom {
        pane: u64,
        mode: Zoom,
        #[serde(default)]
        client: Option<u64>,
    },
    Resize {
        pane: u64,
        axis: Axis,
        ratio: u16,
    },
    Restart {
        pane: u64,
    },
    Read {
        pane: u64,
        #[serde(default)]
        lines: Option<u16>,
        #[serde(default)]
        ansi: bool,
    },
    SendText {
        pane: u64,
        text: String,
    },
    SendKeys {
        pane: u64,
        keys: Vec<String>,
    },
    Wait {
        pane: u64,
        until: Vec<String>,
        #[serde(default)]
        timeout_ms: Option<u64>,
    },
    WaitOutput {
        pane: u64,
        pattern: String,
        #[serde(default)]
        regex: bool,
        #[serde(default)]
        timeout_ms: Option<u64>,
    },
    Subscribe {
        #[serde(default)]
        pane: Option<u64>,
        #[serde(default)]
        timeout_ms: Option<u64>,
        #[serde(default)]
        count: Option<u32>,
    },
}
fn half() -> u16 {
    500
}
impl Request {
    pub fn native(&self) -> bool {
        matches!(
            self,
            Self::AgentPrompt { .. }
                | Self::AgentWait { .. }
                | Self::AgentTurn { .. }
                | Self::AgentStop { .. }
        )
    }
    pub fn monitored(&self) -> bool {
        matches!(
            self,
            Self::Wait { .. } | Self::WaitOutput { .. } | Self::Subscribe { .. }
        )
    }
    pub fn client(&self) -> Option<u64> {
        match self {
            Self::Focus { client, .. }
            | Self::Zoom { client, .. }
            | Self::AgentFocus { client, .. } => *client,
            _ => None,
        }
    }
    pub fn changes_view(&self) -> bool {
        matches!(
            self,
            Self::Focus { .. }
                | Self::AgentFocus { .. }
                | Self::Zoom { .. }
                | Self::CreateSpace { focus: true, .. }
                | Self::WorktreeCreate { focus: true, .. }
                | Self::WorktreeOpen { focus: true, .. }
                | Self::CreateTab { focus: true, .. }
                | Self::SplitPane { focus: true, .. }
        )
    }
    pub fn mutates(&self) -> bool {
        !matches!(
            self,
            Self::Snapshot
                | Self::List { .. }
                | Self::Get { .. }
                | Self::WorktreeList { .. }
                | Self::Read { .. }
                | Self::AgentList
                | Self::AgentGet { .. }
                | Self::AgentRead { .. }
        )
    }
}
pub fn key_bytes(keys: &[String]) -> Result<Vec<u8>, String> {
    if keys.is_empty() || keys.len() > 64 {
        return Err("Send between one and 64 keys".into());
    }
    let mut bytes = Vec::new();
    for key in keys {
        let chord = Chord::parse(key, false)?;
        let encoded = encode(chord.event());
        if encoded.is_empty() {
            return Err(format!("The pane terminal cannot encode {key:?}"));
        }
        bytes.extend(encoded);
    }
    Ok(bytes)
}

pub const HELP: &str = "Muxer local automation (add --session NAME anywhere):
  muxer completion bash|zsh|fish|powershell
  muxer worktree list [--space ID] | open (--branch NAME | --path PATH) [--space ID] [--focus] | create --branch NAME [--space ID] [--base REF] [--path PATH] [--focus]
  muxer status | api snapshot
  muxer api request '{\"method\":\"snapshot\"}'
  muxer space list|get ID|focus ID|rename ID NAME|close ID
  muxer space create [--cwd PATH] [--name NAME] [--focus]
  muxer tab list [--space ID] | get ID | focus ID | rename ID NAME | close ID
  muxer tab create --space ID [--cwd PATH] [--name NAME] [--focus]
  muxer pane list [--tab ID] | get ID | focus ID | rename ID NAME | close ID
  muxer pane split ID [--direction right|down] [--ratio 0.5] [--cwd PATH] [--name NAME] [--focus]
Creation accepts --solmu, --shell, --command TEXT, or --argv JSON_ARRAY.
  muxer pane swap ID OTHER | zoom ID on|off|toggle [--client ID]
  muxer pane resize ID --direction right|down --ratio 0.6
  muxer pane restart ID | read ID [--lines N] [--ansi] [--json]
  muxer pane send-text ID TEXT | send-keys ID KEY [KEY ...]
  muxer pane wait ID --until idle|working|error|exited|shell|running [--until STATE]... [--timeout MS]
  muxer pane wait-output ID --match TEXT|--regex PATTERN [--timeout MS]
  muxer events [--pane ID] [--timeout MS] [--count N]
  muxer agent list | get TARGET | read TARGET [--json] | focus TARGET [--client ID]
  muxer agent rename TARGET NAME|--clear | send-keys TARGET KEY [KEY ...]
  muxer agent prompt TARGET TEXT [--wait] [--timeout MS]
  muxer agent wait TARGET [--turn UUID] [--timeout MS] | turn TARGET UUID | stop TARGET
  muxer terminal attach TARGET [--takeover] | agent attach TARGET [--takeover]
  muxer terminal session control TARGET [--takeover] [--cols N] [--rows N]
  muxer terminal session observe TARGET [--cols N] [--rows N]
Direct attachment uses Ctrl+b q to detach and Ctrl+b Ctrl+b for a literal prefix.
Terminal sessions stream JSON frames; control reads JSON input/resize/scroll/release.
Agent targets are pane IDs or unique live pane names. Prompts queue while busy;
turn waits track the exact submitted turn and preserve terminal drafts.
Focus commands accept --client ID; otherwise they change the server's next-attach view.
New spaces, tabs, and splits preserve focus unless --focus is supplied.
Names can be cleared with rename ID --clear. JSON success goes to stdout;
JSON errors go to stderr (exit 1 for runtime errors, 2 for invalid arguments).
Screen reads are plain text unless --json is supplied. No command starts a server.";

struct Args {
    values: Vec<String>,
    options: bool,
}
impl Args {
    fn agent_target(&mut self) -> Result<AgentTarget, String> {
        let value = self.pop()?;
        if value.bytes().all(|b| b.is_ascii_digit()) {
            Ok(AgentTarget::Id(number(&value)?))
        } else if value.trim().is_empty()
            || value.chars().count() > 80
            || value.chars().any(char::is_control)
        {
            Err("Agent target must be a pane ID or live pane name".into())
        } else {
            Ok(AgentTarget::Name(value))
        }
    }
    fn timeout(&mut self) -> Result<Option<u64>, String> {
        self.take("--timeout")?
            .map(|v| {
                number(&v).and_then(|n| {
                    if n <= 86400000 {
                        Ok(n)
                    } else {
                        Err("Timeout must be 1 through 86400000 milliseconds".into())
                    }
                })
            })
            .transpose()
    }
    fn option_index(&self, option: &str) -> Option<usize> {
        self.options
            .then(|| {
                self.values
                    .iter()
                    .take_while(|v| v.as_str() != "--")
                    .position(|v| v == option)
            })
            .flatten()
    }
    fn take(&mut self, option: &str) -> Result<Option<String>, String> {
        let Some(index) = self.option_index(option) else {
            return Ok(None);
        };
        self.values.remove(index);
        if index >= self.values.len() || self.values[index].starts_with("--") {
            return Err(format!("{option} requires a value"));
        }
        Ok(Some(self.values.remove(index)))
    }
    fn flag(&mut self, option: &str) -> bool {
        self.option_index(option).is_some_and(|i| {
            self.values.remove(i);
            true
        })
    }
    fn pop(&mut self) -> Result<String, String> {
        if self.options && self.values.first().is_some_and(|v| v == "--") {
            self.values.remove(0);
            self.options = false;
        }
        if self.values.is_empty() {
            return Err("Missing argument; use muxer api --help".into());
        }
        Ok(self.values.remove(0))
    }
    fn id(&mut self) -> Result<u64, String> {
        number(&self.pop()?)
    }
    fn optional_id(&mut self, option: &str) -> Result<Option<u64>, String> {
        self.take(option)?.map(|value| number(&value)).transpose()
    }
    fn cwd(&mut self) -> Result<Option<PathBuf>, String> {
        self.take("--cwd")?
            .map(|value| {
                let path = PathBuf::from(value);
                std::path::absolute(path).map_err(|error| error.to_string())
            })
            .transpose()
    }
    fn axis(&mut self) -> Result<Axis, String> {
        match self.take("--direction")?.as_deref().unwrap_or("right") {
            "right" => Ok(Axis::Right),
            "down" => Ok(Axis::Down),
            _ => Err("Direction must be right or down".into()),
        }
    }
    fn ratio(&mut self, required: bool) -> Result<u16, String> {
        let text = self.take("--ratio")?;
        if required && text.is_none() {
            return Err("--ratio is required".into());
        }
        let value: f64 = text
            .as_deref()
            .unwrap_or("0.5")
            .parse()
            .map_err(|_| "Ratio must be 0.1 through 0.9")?;
        if !value.is_finite() || !(0.1..=0.9).contains(&value) {
            return Err("Ratio must be 0.1 through 0.9".into());
        }
        Ok((value * 1000.0).round() as u16)
    }
    fn done(self) -> Result<(), String> {
        if self.values.is_empty() {
            Ok(())
        } else {
            Err(format!("Unexpected argument {:?}", self.values[0]))
        }
    }
}
fn number(value: &str) -> Result<u64, String> {
    value
        .parse()
        .ok()
        .filter(|id| *id > 0)
        .ok_or_else(|| "IDs must be positive integers".into())
}
fn parse(args: &mut Args, group: &str) -> Result<Request, String> {
    if group == "worktree" {
        let action = args.pop()?;
        if action == "list" {
            return Ok(Request::WorktreeList {
                space: args.optional_id("--space")?,
            });
        }
        if action == "open" {
            let branch = args.take("--branch")?;
            let path = args
                .take("--path")?
                .map(PathBuf::from)
                .map(|path| std::path::absolute(path).map_err(|error| error.to_string()))
                .transpose()?;
            if branch.is_some() == path.is_some() {
                return Err("Choose exactly one of --branch or --path".into());
            }
            return Ok(Request::WorktreeOpen {
                space: args.optional_id("--space")?,
                branch,
                path,
                focus: args.flag("--focus"),
            });
        }
        if action != "create" {
            return Err("Use worktree list or create".into());
        }
        let branch = args.take("--branch")?.ok_or("--branch is required")?;
        let base = args.take("--base")?;
        let path = args
            .take("--path")?
            .map(PathBuf::from)
            .map(|path| std::path::absolute(path).map_err(|error| error.to_string()))
            .transpose()?;
        return Ok(Request::WorktreeCreate {
            space: args.optional_id("--space")?,
            branch,
            base,
            path,
            focus: args.flag("--focus"),
        });
    }
    if group == "terminal" {
        let action = args.pop()?;
        let observe = if action == "session" {
            match args.pop()?.as_str() {
                "control" => false,
                "observe" => true,
                _ => return Err("Use terminal session control or observe".into()),
            }
        } else if action == "attach" {
            false
        } else {
            return Err("Use terminal attach or terminal session control/observe".into());
        };
        return terminal_options(args, observe);
    }
    if group == "agent" {
        let action = args.pop()?;
        if action == "list" {
            return Ok(Request::AgentList);
        }
        if action == "attach" {
            return terminal_options(args, false);
        }
        let target = args.agent_target()?;
        return match action.as_str() {
            "get" => Ok(Request::AgentGet { target }),
            "prompt" => {
                let wait = args.flag("--wait");
                let timeout_ms = args.timeout()?;
                let text = args.pop()?;
                solmu_client::automation::validate_text(&text)?;
                Ok(Request::AgentPrompt {
                    target,
                    text,
                    wait,
                    timeout_ms,
                })
            }
            "wait" => Ok(Request::AgentWait {
                target,
                turn: args
                    .take("--turn")?
                    .map(|id| {
                        uuid::Uuid::parse_str(&id)
                            .map(|id| id.to_string())
                            .map_err(|_| "Invalid turn ID".to_string())
                    })
                    .transpose()?,
                timeout_ms: args.timeout()?,
            }),
            "turn" => {
                let id = args.pop()?;
                let turn = uuid::Uuid::parse_str(&id)
                    .map_err(|_| "Invalid turn ID")?
                    .to_string();
                Ok(Request::AgentTurn { target, turn })
            }
            "stop" => Ok(Request::AgentStop { target }),
            "rename" => Ok(Request::AgentRename {
                target,
                name: if args.flag("--clear") {
                    None
                } else {
                    Some(args.pop()?)
                },
            }),
            "focus" => Ok(Request::AgentFocus {
                target,
                client: args.optional_id("--client")?,
            }),
            "read" => Ok(Request::AgentRead {
                target,
                lines: args
                    .take("--lines")?
                    .map(|v| {
                        v.parse()
                            .map_err(|_| "Lines must be 1 through 100".to_string())
                    })
                    .transpose()?,
                ansi: args.flag("--ansi"),
            }),
            "send-keys" => {
                if args.values.first().is_some_and(|v| v == "--") {
                    args.values.remove(0);
                }
                let keys = std::mem::take(&mut args.values);
                key_bytes(&keys)?;
                Ok(Request::AgentKeys { target, keys })
            }
            _ => Err("Unknown agent command".into()),
        };
    }
    if group == "events" {
        return Ok(Request::Subscribe {
            pane: args.optional_id("--pane")?,
            timeout_ms: args.timeout()?,
            count: args
                .take("--count")?
                .map(|v| {
                    number(&v).and_then(|n| {
                        u32::try_from(n).map_err(|_| "Event count is too large".into())
                    })
                })
                .transpose()?,
        });
    }
    if group == "status" {
        return Ok(Request::Snapshot);
    }
    let action = args.pop()?;
    if group == "api" {
        return match action.as_str() {
            "snapshot" => Ok(Request::Snapshot),
            "request" => serde_json::from_str(&args.pop()?)
                .map_err(|e| format!("Invalid control request: {e}")),
            _ => Err("Use api snapshot or api request JSON".into()),
        };
    }
    let target = match group {
        "space" => Kind::Space,
        "tab" => Kind::Tab,
        _ => Kind::Pane,
    };
    if action == "list" {
        let parent = match target {
            Kind::Space => None,
            Kind::Tab => args.optional_id("--space")?,
            Kind::Pane => args.optional_id("--tab")?,
        };
        return Ok(Request::List { target, parent });
    }
    if action == "create" {
        let launch = launch_options(args)?;
        let cwd = args.cwd()?;
        let name = args.take("--name")?;
        let focus = args.flag("--focus");
        if args.flag("--no-focus") && focus {
            return Err("Choose --focus or --no-focus".into());
        }
        return match target {
            Kind::Space => Ok(Request::CreateSpace {
                cwd,
                name,
                focus,
                launch,
            }),
            Kind::Tab => Ok(Request::CreateTab {
                space: args
                    .optional_id("--space")?
                    .ok_or("--space ID is required")?,
                cwd,
                name,
                focus,
                launch,
            }),
            Kind::Pane => Err("Use pane split ID to create a pane".into()),
        };
    }
    let id = args.id()?;
    match action.as_str() {
        "get" => Ok(Request::Get { target, id }),
        "focus" => Ok(Request::Focus {
            target,
            id,
            client: args.optional_id("--client")?,
        }),
        "rename" => Ok(Request::Rename {
            target,
            id,
            name: if args.flag("--clear") {
                None
            } else {
                Some(args.pop()?)
            },
        }),
        "close" => Ok(Request::Close { target, id }),
        _ if !matches!(target, Kind::Pane) => Err("Unknown space or tab command".into()),
        "split" => {
            let launch = launch_options(args)?;
            let axis = args.axis()?;
            let ratio = args.ratio(false)?;
            let cwd = args.cwd()?;
            let name = args.take("--name")?;
            let focus = args.flag("--focus");
            if args.flag("--no-focus") && focus {
                return Err("Choose --focus or --no-focus".into());
            }
            Ok(Request::SplitPane {
                pane: id,
                axis,
                ratio,
                cwd,
                name,
                focus,
                launch,
            })
        }
        "swap" => Ok(Request::Swap {
            pane: id,
            other: args.id()?,
        }),
        "zoom" => {
            let mode = match args.pop()?.as_str() {
                "on" => Zoom::On,
                "off" => Zoom::Off,
                "toggle" => Zoom::Toggle,
                _ => return Err("Zoom must be on, off, or toggle".into()),
            };
            Ok(Request::Zoom {
                pane: id,
                mode,
                client: args.optional_id("--client")?,
            })
        }
        "resize" => Ok(Request::Resize {
            pane: id,
            axis: args.axis()?,
            ratio: args.ratio(true)?,
        }),
        "restart" => Ok(Request::Restart { pane: id }),
        "read" => Ok(Request::Read {
            pane: id,
            lines: args
                .take("--lines")?
                .map(|v| {
                    v.parse::<u16>()
                        .map_err(|_| "Lines must be 1 through 100".to_string())
                })
                .transpose()?,
            ansi: args.flag("--ansi"),
        }),
        "send-text" => Ok(Request::SendText {
            pane: id,
            text: args.pop()?,
        }),
        "send-keys" => {
            if args.values.first().is_some_and(|v| v == "--") {
                args.values.remove(0);
            }
            let keys = std::mem::take(&mut args.values);
            key_bytes(&keys)?;
            Ok(Request::SendKeys { pane: id, keys })
        }
        "wait" => {
            let mut until = Vec::new();
            while let Some(value) = args.take("--until")? {
                until.push(value);
            }
            if until.is_empty()
                || until.iter().any(|s| {
                    !matches!(
                        s.as_str(),
                        "idle" | "working" | "error" | "exited" | "shell" | "running"
                    )
                })
            {
                return Err(
                    "Choose --until idle, working, error, exited, shell, or running".into(),
                );
            }
            Ok(Request::Wait {
                pane: id,
                until,
                timeout_ms: args.timeout()?,
            })
        }
        "wait-output" => {
            let text = args.take("--match")?;
            let regex = args.take("--regex")?;
            let is_regex = regex.is_some();
            if text.is_some() == is_regex {
                return Err("Choose exactly one of --match TEXT or --regex PATTERN".into());
            }
            let pattern = text.or(regex).unwrap();
            crate::monitor::pattern(&pattern, is_regex)?;
            Ok(Request::WaitOutput {
                pane: id,
                pattern,
                regex: is_regex,
                timeout_ms: args.timeout()?,
            })
        }
        _ => Err("Unknown pane command".into()),
    }
}
fn failure(error: impl std::fmt::Display, code: i32) -> ! {
    eprintln!("{}", json!({"ok": false, "error": error.to_string()}));
    std::process::exit(code);
}
fn launch_options(args: &mut Args) -> Result<Option<crate::launch::Launch>, String> {
    use crate::launch::Launch;
    let solmu = args.flag("--solmu");
    let shell = args.flag("--shell");
    let command = args.take("--command")?;
    let argv = args.take("--argv")?;
    if usize::from(solmu)
        + usize::from(shell)
        + usize::from(command.is_some())
        + usize::from(argv.is_some())
        > 1
    {
        return Err("Choose one of --solmu, --shell, --command, or --argv".into());
    }
    let launch = if solmu {
        Some(Launch::Solmu)
    } else if shell {
        Some(Launch::Shell { argv: vec![] })
    } else if let Some(command) = command {
        Some(Launch::script(&command)?)
    } else if let Some(argv) = argv {
        let argv = serde_json::from_str::<Vec<String>>(&argv)
            .map_err(|_| "--argv must be a JSON array of strings")?;
        let launch = Launch::Command { argv };
        launch.validate()?;
        Some(launch)
    } else {
        None
    };
    Ok(launch)
}
fn terminal_options(args: &mut Args, observe: bool) -> Result<Request, String> {
    let target = args.agent_target()?;
    let takeover = args.flag("--takeover");
    let cols = args
        .take("--cols")?
        .map(|v| v.parse::<u16>().map_err(|_| "Invalid columns".to_owned()))
        .transpose()?;
    let rows = args
        .take("--rows")?
        .map(|v| v.parse::<u16>().map_err(|_| "Invalid rows".to_owned()))
        .transpose()?;
    crate::terminal::validate_size(cols, rows)?;
    if observe && takeover {
        return Err("Observers cannot take over a terminal".into());
    }
    Ok(Request::TerminalOpen {
        target,
        observe,
        takeover,
        cols,
        rows,
    })
}
/// Returns false for the existing interactive/server/session command families.
pub fn cli(values: Vec<OsString>) -> bool {
    let mut values = values;
    let mut name = "default".to_owned();
    // Recognize global session selectors before or after the command group.
    let mut i = 0;
    while i < values.len() {
        if values[i] == "--" {
            break;
        }
        if values[i] == "--session" {
            if i + 1 >= values.len() {
                break;
            }
            name = values.remove(i + 1).to_string_lossy().into_owned();
            values.remove(i);
        } else {
            i += 1;
        }
    }
    let Some(group) = values.first().and_then(|v| v.to_str()) else {
        return false;
    };
    if group == "completion" {
        let Some(shell) = values.get(1).and_then(|value| value.to_str()) else {
            failure("Usage: muxer completion bash|zsh|fish|powershell", 2);
        };
        if values.len() != 2 {
            failure("Usage: muxer completion bash|zsh|fish|powershell", 2);
        }
        match completion(shell) {
            Ok(script) => print!("{script}"),
            Err(error) => failure(error, 2),
        }
        return true;
    }
    if !matches!(
        group,
        "space" | "tab" | "pane" | "api" | "status" | "events" | "agent" | "terminal" | "worktree"
    ) {
        return false;
    }
    let group = group.to_owned();
    let mut args = Args {
        options: true,
        values: values
            .into_iter()
            .map(|v| {
                v.into_string()
                    .unwrap_or_else(|_| failure("Arguments must be valid UTF-8", 2))
            })
            .collect(),
    };
    args.pop().unwrap();
    if args.flag("--help") || args.flag("-h") {
        println!("{HELP}");
        return true;
    }
    let json_output = args.flag("--json");
    let interactive = group != "api" && args.values.first().is_some_and(|v| v == "attach");
    let request = parse(&mut args, &group).unwrap_or_else(|e| failure(e, 2));
    args.done().unwrap_or_else(|e| failure(e, 2));
    session::validate_name(&name).unwrap_or_else(|e| failure(e, 2));
    if let Err(e) = dotenvy::dotenv()
        && !e.not_found()
    {
        failure("Could not load .env", 1);
    }
    let text = matches!(request, Request::Read { .. } | Request::AgentRead { .. }) && !json_output;
    if matches!(request, Request::TerminalOpen { .. }) {
        crate::terminal::run(&name, request, interactive).unwrap_or_else(|e| runtime_failure(e));
        return true;
    }
    if matches!(request, Request::Subscribe { .. }) {
        session::subscribe(&name, request).unwrap_or_else(|e| failure(e, 1));
        return true;
    }
    let result = session::rpc(&name, request).unwrap_or_else(|error| runtime_failure(error));
    if text {
        print!("{}", result["text"].as_str().unwrap_or_default());
    } else {
        println!("{}", success(result));
    }
    true
}

fn completion(shell: &str) -> Result<&'static str, &'static str> {
    match shell {
        "bash" => Ok(BASH_COMPLETION),
        "zsh" => Ok(ZSH_COMPLETION),
        "fish" => Ok(FISH_COMPLETION),
        "powershell" | "pwsh" => Ok(POWERSHELL_COMPLETION),
        _ => Err("Choose bash, zsh, fish, or powershell"),
    }
}

const BASH_COMPLETION: &str = r#"_muxer_completions() {
  local cur group action candidates
  cur="${COMP_WORDS[COMP_CWORD]}"
  group="${COMP_WORDS[1]}"
  action="${COMP_WORDS[2]}"
  case "$group:$action" in
    space:*) candidates="list get create focus rename close" ;;
    tab:*) candidates="list get create focus rename close" ;;
    pane:*) candidates="list get split focus rename close swap zoom resize restart read send-text send-keys wait wait-output" ;;
    agent:*) candidates="list get read focus rename send-keys prompt wait turn stop" ;;
    terminal:*) candidates="attach session" ;;
    server:*) candidates="start status stop" ;;
    session:*) candidates="list attach" ;;
    events:*) candidates="--pane --timeout --count --json" ;;
    status:*) candidates="server" ;;
    api:*) candidates="snapshot request" ;;
    completion:*) candidates="bash zsh fish powershell" ;;
    worktree:*) candidates="create" ;;
    *) candidates="space tab pane api status events agent terminal session server worktree completion --help --version --default-config --session --cwd --foreground" ;;
  esac
  COMPREPLY=( $(compgen -W "$candidates" -- "$cur") )
}
complete -F _muxer_completions muxer
"#;

const ZSH_COMPLETION: &str = r#"#compdef muxer
local -a commands
commands=(space tab pane api status events agent terminal session server worktree completion)
if (( CURRENT == 2 )); then
  _describe 'command' commands
else
  case "$words[2]" in
    space|tab) _values 'action' list get create focus rename close ;;
    pane) _values 'action' list get split focus rename close swap zoom resize restart read send-text send-keys wait wait-output ;;
    agent) _values 'action' list get read focus rename send-keys prompt wait turn stop ;;
    terminal) _values 'action' attach session ;;
    server) _values 'action' start status stop ;;
    session) _values 'action' list attach ;;
    status) _values 'action' server ;;
    api) _values 'action' snapshot request ;;
    completion) _values 'shell' bash zsh fish powershell ;;
    worktree) _values 'action' create ;;
  esac
fi
"#;

const FISH_COMPLETION: &str = r#"complete -c muxer -f -n '__fish_use_subcommand' -a 'space tab pane api status events agent terminal session server worktree completion'
complete -c muxer -f -n '__fish_seen_subcommand_from space' -a 'list get create focus rename close'
complete -c muxer -f -n '__fish_seen_subcommand_from tab' -a 'list get create focus rename close'
complete -c muxer -f -n '__fish_seen_subcommand_from pane' -a 'list get split focus rename close swap zoom resize restart read send-text send-keys wait wait-output'
complete -c muxer -f -n '__fish_seen_subcommand_from agent' -a 'list get read focus rename send-keys prompt wait turn stop'
complete -c muxer -f -n '__fish_seen_subcommand_from terminal' -a 'attach session'
complete -c muxer -f -n '__fish_seen_subcommand_from server' -a 'start status stop'
complete -c muxer -f -n '__fish_seen_subcommand_from session' -a 'list attach'
complete -c muxer -f -n '__fish_seen_subcommand_from status' -a 'server'
complete -c muxer -f -n '__fish_seen_subcommand_from api' -a 'snapshot request'
complete -c muxer -f -n '__fish_seen_subcommand_from completion' -a 'bash zsh fish powershell'
complete -c muxer -f -n '__fish_seen_subcommand_from worktree' -a 'create'
"#;

const POWERSHELL_COMPLETION: &str = r#"Register-ArgumentCompleter -Native -CommandName muxer -ScriptBlock {
  param($wordToComplete, $commandAst, $cursorPosition)
  $words = @($commandAst.CommandElements | ForEach-Object { $_.ToString() })
  $candidates = switch ($words.Count) {
    1 { 'space tab pane api status events agent terminal session server worktree completion --help --version --default-config --session --cwd --foreground' }
    default {
      switch ($words[1]) {
        { $_ -in 'space', 'tab' } { 'list get create focus rename close' }
        'pane' { 'list get split focus rename close swap zoom resize restart read send-text send-keys wait wait-output' }
        'agent' { 'list get read focus rename send-keys prompt wait turn stop' }
        'terminal' { 'attach session' }
        'server' { 'start status stop' }
        'session' { 'list attach' }
        'status' { 'server' }
        'api' { 'snapshot request' }
        'completion' { 'bash zsh fish powershell' }
        'worktree' { 'create' }
        default { '' }
      }
    }
  }
  $candidates -split ' ' | Where-Object { $_ -like "$wordToComplete*" } | ForEach-Object {
    [System.Management.Automation.CompletionResult]::new($_, $_, 'ParameterValue', $_)
  }
}
"#;

pub fn success(result: Value) -> Value {
    json!({"ok": true, "result": result})
}
fn runtime_failure(error: Box<dyn std::error::Error>) -> ! {
    if let Some(error) = error.downcast_ref::<crate::agent::Failure>() {
        eprintln!(
            "{}",
            json!({"ok":false,"error":error.failure.message,"code":error.failure.code,"accepted":error.failure.accepted,"turn":error.failure.turn,"pane":error.pane,"instance":error.instance,"thread":error.thread})
        );
        std::process::exit(1);
    }
    failure(error, 1)
}
