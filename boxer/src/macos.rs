use crate::Policy;
use std::{io, os::unix::process::CommandExt, process::Command};

pub fn run(command: Command, policy: Policy) -> io::Result<i32> {
    let profile = if policy.read_only {
        "(version 1)(allow default)(deny file-write*)"
    } else {
        "(version 1)(allow default)"
    };
    let mut sandbox = Command::new("/usr/bin/sandbox-exec");
    sandbox
        .args(["-p", profile, "--"])
        .arg(command.get_program())
        .args(command.get_args());
    if let Some(directory) = command.get_current_dir() {
        sandbox.current_dir(directory);
    }
    // sandbox-exec applies Seatbelt to this process tree. Missing native support
    // produces an error; there is no unprotected fallback.
    Err(sandbox.exec())
}
