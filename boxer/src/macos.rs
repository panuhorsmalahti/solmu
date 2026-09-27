use crate::{
    Policy,
    policy::{self, Mode},
};
use std::{io, os::unix::process::CommandExt, process::Command};

pub fn run(command: Command, policy: Policy) -> io::Result<i32> {
    let profile = if policy.mode == Mode::Workspace {
        let executable = policy::executable(&command)?;
        let mut read = policy::runtime_paths();
        read.extend(policy.read.clone());
        read.push(
            command
                .get_current_dir()
                .expect("resolved workspace")
                .to_owned(),
        );
        read.push(executable);
        let mut write = policy::device_paths();
        if !policy.read_only {
            write.push(
                command
                    .get_current_dir()
                    .expect("resolved workspace")
                    .to_owned(),
            );
        }
        write.extend(policy.write.clone());
        read.extend(write.clone());
        let mut profile = String::from(
            "(version 1)(deny default)(allow process*)(allow signal)(allow sysctl-read)(allow mach-lookup)(allow network*)(allow file-read-metadata)",
        );
        for (operation, paths) in [
            ("file-read* file-map-executable", read),
            ("file-write*", write),
        ] {
            for path in paths {
                let kind = if path.is_dir() { "subpath" } else { "literal" };
                let path = path
                    .to_str()
                    .ok_or_else(|| io::Error::other("Seatbelt paths must be UTF-8"))?;
                profile.push_str(&format!("(allow {operation} ({kind} {}))", quote(path)));
            }
        }
        profile
    } else if policy.read_only {
        "(version 1)(allow default)(deny file-write*)".to_owned()
    } else {
        "(version 1)(allow default)".to_owned()
    };
    let mut sandbox = Command::new("/usr/bin/sandbox-exec");
    sandbox
        .arg("-p")
        .arg(profile)
        .arg("--")
        .arg(command.get_program())
        .args(command.get_args());
    if let Some(directory) = command.get_current_dir() {
        sandbox.current_dir(directory);
    }
    if policy.clean_env {
        sandbox.env_clear();
    }
    for (name, value) in command.get_envs() {
        if let Some(value) = value {
            sandbox.env(name, value);
        } else {
            sandbox.env_remove(name);
        }
    }
    // sandbox-exec applies Seatbelt to this process tree. Missing native support
    // produces an error; there is no unprotected fallback.
    Err(sandbox.exec())
}

fn quote(path: &str) -> String {
    let mut quoted = String::from("\"");
    for ch in path.chars() {
        match ch {
            '\\' => quoted.push_str("\\\\"),
            '"' => quoted.push_str("\\\""),
            '\n' => quoted.push_str("\\n"),
            '\r' => quoted.push_str("\\r"),
            '\t' => quoted.push_str("\\t"),
            _ => quoted.push(ch),
        }
    }
    quoted.push('"');
    quoted
}
