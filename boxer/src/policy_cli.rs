use crate::{Mode, Policy};
use serde_json::{Value, json};
use std::{
    ffi::OsString,
    io,
    path::{Path, PathBuf},
};

const PROFILES: &[(&str, &str)] = &[
    (
        "solmu",
        "Solmu workspace profile with a filtered environment",
    ),
    ("codex", "Codex with separate Boxer-managed login state"),
    (
        "claude-code",
        "Claude Code with separate Boxer-managed login state",
    ),
    (
        "opencode",
        "OpenCode with private config, data, cache, log, and state",
    ),
    ("pi", "Pi with a separate Boxer-managed agent directory"),
];

pub fn command(args: &[OsString]) -> io::Result<i32> {
    match args.get(1).and_then(|arg| arg.to_str()) {
        Some("profiles") if args.len() == 2 => {
            println!(
                "{}",
                serde_json::to_string_pretty(
                    &PROFILES
                        .iter()
                        .map(|(name, description)| json!({"name":name,"description":description}))
                        .collect::<Vec<_>>()
                )
                .map_err(io::Error::other)?
            );
            Ok(0)
        }
        Some("validate") if args.len() >= 3 => {
            let (file, workspace) = parse_file_and_cwd(args, 2)?;
            let policy = load(&file, &workspace)?;
            println!("{}", serde_json::to_string_pretty(&json!({"valid":true,"platform":std::env::consts::OS,"platform_supported":supported(&policy),"workspace":workspace,"policy":policy})).map_err(io::Error::other)?);
            Ok(0)
        }
        Some("show") if args.len() >= 3 => {
            let (file, workspace) = parse_file_and_cwd(args, 2)?;
            let policy = load(&file, &workspace)?;
            println!("{}", serde_json::to_string_pretty(&json!({"platform":std::env::consts::OS,"platform_supported":supported(&policy),"workspace":workspace,"policy":policy})).map_err(io::Error::other)?);
            Ok(0)
        }
        Some("diff") if args.len() >= 4 => {
            let before_path = PathBuf::from(&args[2]);
            let after_path = PathBuf::from(&args[3]);
            let mut workspace = std::env::current_dir()?;
            let mut index = 4;
            while index < args.len() {
                if args[index] == "--cwd" && index + 1 < args.len() {
                    workspace = PathBuf::from(&args[index + 1]);
                    index += 2;
                } else {
                    return Err(usage());
                }
            }
            let workspace = workspace.canonicalize()?;
            let before =
                serde_json::to_value(load(&before_path, &workspace)?).map_err(io::Error::other)?;
            let after =
                serde_json::to_value(load(&after_path, &workspace)?).map_err(io::Error::other)?;
            let mut changes = serde_json::Map::new();
            if let (Value::Object(before), Value::Object(after)) = (&before, &after) {
                for key in before.keys().chain(after.keys()) {
                    if before.get(key) != after.get(key) {
                        changes.insert(
                            key.clone(),
                            json!({"before":before.get(key),"after":after.get(key)}),
                        );
                    }
                }
            }
            println!(
                "{}",
                serde_json::to_string_pretty(
                    &json!({"before":before_path,"after":after_path,"changes":changes})
                )
                .map_err(io::Error::other)?
            );
            Ok(0)
        }
        _ => Err(usage()),
    }
}

fn parse_file_and_cwd(args: &[OsString], file_index: usize) -> io::Result<(PathBuf, PathBuf)> {
    let file = PathBuf::from(&args[file_index]);
    let mut workspace = std::env::current_dir()?;
    let mut index = file_index + 1;
    while index < args.len() {
        if args[index] == "--cwd" && index + 1 < args.len() {
            workspace = PathBuf::from(&args[index + 1]);
            index += 2;
        } else {
            return Err(usage());
        }
    }
    Ok((file, workspace.canonicalize()?))
}

fn load(file: &Path, workspace: &Path) -> io::Result<Policy> {
    let mut policy = Policy::from_file(file)?;
    policy.resolve(workspace)?;
    Ok(policy)
}

fn supported(policy: &Policy) -> bool {
    if cfg!(windows) {
        policy.mode == Mode::Unrestricted
            && !policy.read_only
            && policy.network == crate::Network::Allow
    } else if cfg!(target_os = "macos") {
        !policy.isolated && policy.network != crate::Network::Proxy
    } else {
        cfg!(target_os = "linux")
            && ((policy.network == crate::Network::Allow && !policy.isolated)
                || cfg!(any(target_arch = "x86_64", target_arch = "aarch64")))
    }
}

fn usage() -> io::Error {
    io::Error::other(
        "Usage: boxer policy profiles | validate FILE [--cwd PATH] | show FILE [--cwd PATH] | diff BEFORE AFTER [--cwd PATH]",
    )
}
