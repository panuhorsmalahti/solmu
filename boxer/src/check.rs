use crate::Policy;
#[cfg(any(target_os = "linux", target_os = "macos"))]
use crate::policy::Network;
use std::{
    io::{self, Read, Write},
    path::Path,
    process::{Command, Stdio},
};

const WORKER: &str = "--boxer-check-worker";

pub fn run(policy: &Policy, workspace: &Path) -> io::Result<i32> {
    // Send the already resolved policy rather than reopening the user's file.
    // No credentials or environment values are included in this transport.
    let source = serde_json::to_vec(policy).map_err(io::Error::other)?;
    if source.len() > 1_000_000 {
        return Err(io::Error::other("Readiness policy exceeds 1 MB"));
    }
    // Validate inherited socket I/O in the caller too: the worker uses pipes.
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    if policy.network == Network::Deny {
        crate::unix::prepare_network_denial()?;
    }
    let mut command = Command::new(std::env::current_exe()?);
    command
        .arg(WORKER)
        .current_dir(workspace)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    policy.environment(&mut command);
    let mut worker = command.spawn()?;
    let sent = worker.stdin.take().unwrap().write_all(&source);
    let output = worker.wait_with_output()?;
    if !output.status.success() {
        return Err(io::Error::other(format!(
            "Policy readiness check failed ({}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    sent?;
    if String::from_utf8_lossy(&output.stdout).trim()
        != format!("Solmu Boxer {}", env!("CARGO_PKG_VERSION"))
    {
        return Err(io::Error::other("Policy check did not complete its probe"));
    }
    println!(
        "{}",
        serde_json::json!({
            "platform": std::env::consts::OS,
            "workspace": workspace,
            "mode": policy.mode,
            "network": policy.network,
            "enforcement": "checked",
            "program_started": false
        })
    );
    Ok(0)
}

pub fn worker() -> io::Result<Option<i32>> {
    let mut arguments = std::env::args_os().skip(1);
    if arguments.next().as_deref() != Some(std::ffi::OsStr::new(WORKER)) {
        return Ok(None);
    }
    if arguments.next().is_some() {
        return Err(io::Error::other("Invalid readiness worker invocation"));
    }
    let mut source = Vec::new();
    io::stdin().take(1_000_001).read_to_end(&mut source)?;
    if source.len() > 1_000_000 {
        return Err(io::Error::other("Readiness policy exceeds 1 MB"));
    }
    let mut policy: Policy = serde_json::from_slice(&source).map_err(io::Error::other)?;
    let workspace = std::env::current_dir()?.canonicalize()?;
    policy.resolve(&workspace)?;
    #[cfg(not(target_os = "linux"))]
    if policy.isolated {
        return Err(io::Error::other(
            "--isolated is currently supported only on Linux",
        ));
    }
    let mut command = Command::new(std::env::current_exe()?);
    command.arg("--version").current_dir(workspace);
    policy.environment(&mut command);
    crate::platform::run(command, policy).map(Some)
}
