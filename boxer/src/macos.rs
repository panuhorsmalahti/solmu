use crate::{
    Policy,
    policy::{self, Mode, Network},
};
use std::{
    io,
    os::unix::process::{CommandExt, ExitStatusExt},
    process::Command,
};

pub fn run(command: Command, policy: Policy) -> io::Result<i32> {
    if policy.network == Network::Deny {
        crate::unix::prepare_network_denial()?;
    }
    let mut profile = if policy.mode == Mode::Workspace {
        let executable = policy::executable(&command)?;
        let mut read = policy::runtime_paths();
        read.extend(policy.read.clone());
        let mut readable_grants = read.clone();
        readable_grants.extend(policy.write.clone());
        readable_grants.extend(policy::device_paths());
        readable_grants.push(
            command
                .get_current_dir()
                .expect("resolved workspace")
                .to_owned(),
        );
        policy.validate_write_only_overlaps(&readable_grants)?;
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
        let write_only = policy.write_only.clone();
        let mut profile = String::from(
            "(version 1)(deny default)(allow process*)(allow signal)(allow sysctl-read)(allow mach-lookup)(allow file-read-metadata)",
        );
        if policy.network == Network::Allow {
            profile.push_str("(allow network*)");
        }
        for directory in policy::runtime_list() {
            profile.push_str(&format!(
                "(allow file-read* (literal {}))",
                quote(directory.to_str().unwrap())
            ));
        }
        profile.push_str("(allow file-read-data (literal \"/dev/stdin\")(literal \"/dev/fd/0\"))(allow file-write-data (literal \"/dev/stdout\")(literal \"/dev/stderr\")(literal \"/dev/fd/1\")(literal \"/dev/fd/2\"))");
        // Terminal I/O is explicitly inherited. Grant ioctl only for the actual
        // terminal device attached to a standard descriptor, not every host TTY.
        for descriptor in 0..=2 {
            let mut buffer = [0u8; libc::PATH_MAX as usize];
            // SAFETY: F_GETPATH writes at most PATH_MAX bytes to this buffer.
            if unsafe { libc::fcntl(descriptor, libc::F_GETPATH, buffer.as_mut_ptr()) } == 0 {
                let length = buffer
                    .iter()
                    .position(|byte| *byte == 0)
                    .unwrap_or(buffer.len());
                let path = std::str::from_utf8(&buffer[..length]).map_err(io::Error::other)?;
                if path.starts_with("/dev/") {
                    profile.push_str(&format!(
                        "(allow file-read* file-write* file-ioctl (literal {}))",
                        quote(path)
                    ));
                }
            }
        }
        for (operation, paths) in [
            ("file-read* file-map-executable", read),
            ("file-write*", write),
            ("file-write*", write_only),
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
    if policy.network == Network::Deny {
        profile.push_str("(deny network*)(deny system-socket)");
    }
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
    if crate::sessions::is_child() {
        let status = sandbox.status()?;
        return Ok(status
            .code()
            .unwrap_or_else(|| 128 + status.signal().unwrap_or(1)));
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
