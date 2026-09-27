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
#[derive(Deserialize, Serialize)]
#[serde(
    tag = "method",
    content = "params",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum Request {
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
        cwd: Option<PathBuf>,
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        focus: bool,
    },
    CreateTab {
        space: u64,
        #[serde(default)]
        cwd: Option<PathBuf>,
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        focus: bool,
    },
    SplitPane {
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
    pub fn monitored(&self) -> bool {
        matches!(
            self,
            Self::Wait { .. } | Self::WaitOutput { .. } | Self::Subscribe { .. }
        )
    }
    pub fn client(&self) -> Option<u64> {
        match self {
            Self::Focus { client, .. } | Self::Zoom { client, .. } => *client,
            _ => None,
        }
    }
    pub fn changes_view(&self) -> bool {
        matches!(
            self,
            Self::Focus { .. }
                | Self::Zoom { .. }
                | Self::CreateSpace { focus: true, .. }
                | Self::CreateTab { focus: true, .. }
                | Self::SplitPane { focus: true, .. }
        )
    }
    pub fn mutates(&self) -> bool {
        !matches!(
            self,
            Self::Snapshot | Self::List { .. } | Self::Get { .. } | Self::Read { .. }
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
  muxer status | api snapshot
  muxer api request '{\"method\":\"snapshot\"}'
  muxer space list|get ID|focus ID|rename ID NAME|close ID
  muxer space create [--cwd PATH] [--name NAME] [--focus]
  muxer tab list [--space ID] | get ID | focus ID | rename ID NAME | close ID
  muxer tab create --space ID [--cwd PATH] [--name NAME] [--focus]
  muxer pane list [--tab ID] | get ID | focus ID | rename ID NAME | close ID
  muxer pane split ID [--direction right|down] [--ratio 0.5] [--cwd PATH] [--name NAME] [--focus]
  muxer pane swap ID OTHER | zoom ID on|off|toggle [--client ID]
  muxer pane resize ID --direction right|down --ratio 0.6
  muxer pane restart ID | read ID [--lines N] [--ansi] [--json]
  muxer pane send-text ID TEXT | send-keys ID KEY [KEY ...]
  muxer pane wait ID --until idle|working|error|exited [--until STATE]... [--timeout MS]
  muxer pane wait-output ID --match TEXT|--regex PATTERN [--timeout MS]
  muxer events [--pane ID] [--timeout MS] [--count N]
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
        let cwd = args.cwd()?;
        let name = args.take("--name")?;
        let focus = args.flag("--focus");
        if args.flag("--no-focus") && focus {
            return Err("Choose --focus or --no-focus".into());
        }
        return match target {
            Kind::Space => Ok(Request::CreateSpace { cwd, name, focus }),
            Kind::Tab => Ok(Request::CreateTab {
                space: args
                    .optional_id("--space")?
                    .ok_or("--space ID is required")?,
                cwd,
                name,
                focus,
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
                || until
                    .iter()
                    .any(|s| !matches!(s.as_str(), "idle" | "working" | "error" | "exited"))
            {
                return Err("Choose --until idle, working, error, or exited".into());
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
    if !matches!(
        group,
        "space" | "tab" | "pane" | "api" | "status" | "events"
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
    let request = parse(&mut args, &group).unwrap_or_else(|e| failure(e, 2));
    args.done().unwrap_or_else(|e| failure(e, 2));
    session::validate_name(&name).unwrap_or_else(|e| failure(e, 2));
    if let Err(e) = dotenvy::dotenv()
        && !e.not_found()
    {
        failure("Could not load .env", 1);
    }
    let text = matches!(request, Request::Read { .. }) && !json_output;
    if matches!(request, Request::Subscribe { .. }) {
        session::subscribe(&name, request).unwrap_or_else(|e| failure(e, 1));
        return true;
    }
    let result = session::rpc(&name, request).unwrap_or_else(|e| failure(e, 1));
    if text {
        print!("{}", result["text"].as_str().unwrap_or_default());
    } else {
        println!("{}", success(result));
    }
    true
}
pub fn success(result: Value) -> Value {
    json!({"ok": true, "result": result})
}
