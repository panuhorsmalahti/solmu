use crate::Policy;
use landlock::{
    ABI, Access, AccessFs, CompatLevel, Compatible, PathBeneath, PathFd, Ruleset, RulesetAttr,
    RulesetCreatedAttr,
};
use std::{
    io,
    os::{
        fd::AsRawFd,
        unix::process::{CommandExt, ExitStatusExt},
    },
    process::Command,
};

mod cgroup;
mod seccomp;

static INTERRUPTED: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);
extern "C" fn interrupted(signal: i32) {
    INTERRUPTED.store(signal, std::sync::atomic::Ordering::Relaxed);
}

pub fn run(mut command: Command, policy: Policy) -> io::Result<i32> {
    if policy.isolated {
        return isolated(command, policy);
    }
    // The launcher is single-threaded. Apply the irreversible policy before exec,
    // rather than allocating or taking locks in a post-fork pre_exec callback.
    let handled = AccessFs::from_all(ABI::V3);
    let allowed = if policy.read_only {
        AccessFs::from_read(ABI::V3)
    } else {
        handled
    };
    Ruleset::default()
        .set_compatibility(CompatLevel::HardRequirement)
        .handle_access(handled)
        .map_err(io::Error::other)?
        .create()
        .map_err(io::Error::other)?
        .add_rule(PathBeneath::new(
            PathFd::new("/").map_err(io::Error::other)?,
            allowed,
        ))
        .map_err(io::Error::other)?
        .restrict_self()
        .map_err(io::Error::other)?;
    // Network access is deliberately not handled by the ruleset: all requests
    // are allowed. Existing caller permissions still apply.
    Err(command.exec())
}

fn isolated(command: Command, policy: Policy) -> io::Result<i32> {
    let directory = command
        .get_current_dir()
        .map(std::path::PathBuf::from)
        .unwrap_or(std::env::current_dir()?)
        .canonicalize()?;
    if !directory.is_dir()
        || directory == std::path::Path::new("/")
        || ["/sys", "/proc", "/dev"]
            .iter()
            .any(|path| directory.starts_with(path))
    {
        return Err(io::Error::other(
            "--isolated requires a project workspace outside /, /sys, /proc, and /dev",
        ));
    }
    let mut sandbox = Command::new("bwrap");
    sandbox.args([
        "--unshare-user",
        "--unshare-pid",
        "--unshare-ipc",
        "--unshare-uts",
        "--unshare-cgroup",
        "--disable-userns",
        "--cap-drop",
        "ALL",
        "--die-with-parent",
        "--new-session",
        "--hostname",
        "solmu",
        "--clearenv",
    ]);
    // Share only runtime files, not the host's home, /run, or arbitrary mounts.
    for path in [
        "/usr",
        "/bin",
        "/sbin",
        "/lib",
        "/lib64",
        "/etc/resolv.conf",
        "/etc/hosts",
        "/etc/nsswitch.conf",
        "/etc/gai.conf",
        "/etc/ssl",
        "/etc/pki",
        "/etc/fonts",
        "/etc/localtime",
    ] {
        sandbox.args(["--ro-bind-try", path, path]);
    }
    sandbox.args(["--proc", "/proc", "--dev", "/dev", "--tmpfs", "/tmp"]);
    sandbox
        .arg(if policy.read_only {
            "--ro-bind"
        } else {
            "--bind"
        })
        .arg(&directory)
        .arg(&directory);
    sandbox.args([
        "--dir",
        "/tmp/solmu-home",
        "--setenv",
        "HOME",
        "/tmp/solmu-home",
    ]);
    let requested = std::path::Path::new(command.get_program());
    let executable = if requested.is_absolute() {
        requested.to_owned()
    } else if requested.components().count() > 1 {
        directory.join(requested)
    } else {
        std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
            .map(|path| path.join(requested))
            .find(|path| path.is_file())
            .ok_or_else(|| io::Error::other("Agent executable was not found on PATH"))?
    }
    .canonicalize()?;
    sandbox
        .arg("--ro-bind")
        .arg(executable)
        .arg("/opt/solmu/agent");
    // Explicit environment only: provider credentials remain available, while
    // SSH agents and other host service handles are not forwarded.
    for (key, value) in std::env::vars_os() {
        let name = key.to_string_lossy();
        if matches!(
            name.as_ref(),
            "PATH"
                | "TERM"
                | "LANG"
                | "LC_ALL"
                | "COLORTERM"
                | "VERTEX_PROJECT_ID"
                | "VERTEX_LOCATION"
                | "AWS_REGION"
                | "AWS_DEFAULT_REGION"
                | "GITHUB_TOKEN"
                | "HTTP_PROXY"
                | "HTTPS_PROXY"
                | "ALL_PROXY"
                | "NO_PROXY"
                | "http_proxy"
                | "https_proxy"
                | "all_proxy"
                | "no_proxy"
        ) || name.starts_with("LLM_")
            || name.starts_with("SOLMU_")
            || name.ends_with("_API_KEY")
            || name.ends_with("_AUTH_TOKEN")
        {
            sandbox.arg("--setenv").arg(key).arg(value);
        }
    }
    sandbox
        .arg("--chdir")
        .arg(&directory)
        .arg("--")
        .arg("/opt/solmu/agent")
        .args(command.get_args());
    // No network namespace is created: host networking stays available.
    let group = cgroup::Group::create(&policy)?;
    group.validate_workspace(&directory)?;
    let filter = seccomp::filter()?;
    let filter_fd = filter.as_raw_fd();
    let group_fd = group.as_raw_fd();
    // Options must precede the agent separator.
    let mut supervised = Command::new("bwrap");
    supervised
        .arg("--seccomp")
        .arg(filter_fd.to_string())
        .args(sandbox.get_args());
    // SAFETY: callback uses only write/fcntl and errno access; descriptors stay
    // alive until the child has completed, and attachment precedes exec.
    unsafe {
        supervised.pre_exec(move || {
            cgroup::attach(group_fd)?;
            if libc::fcntl(filter_fd, libc::F_SETFD, 0) < 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
    for signal in [libc::SIGINT, libc::SIGTERM, libc::SIGHUP] {
        // SAFETY: the handler only writes a lock-free atomic. The single-threaded
        // launcher installs it before spawning; exec resets caught handlers.
        if unsafe { libc::signal(signal, interrupted as *const () as usize) } == libc::SIG_ERR {
            return Err(io::Error::last_os_error());
        }
    }
    let mut child = supervised.spawn().map_err(|error| {
        io::Error::other(format!(
            "Could not start Bubblewrap with enforced cgroup limits: {error}"
        ))
    })?;
    let mut cancelled = 0;
    let status = loop {
        let signal = INTERRUPTED.load(std::sync::atomic::Ordering::Relaxed);
        if signal != 0 && cancelled == 0 {
            cancelled = signal;
            group.terminate()?;
        }
        if let Some(status) = child.try_wait()? {
            break status;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    };
    group.cleanup()?;
    if cancelled != 0 {
        return Ok(128 + cancelled);
    }
    Ok(status
        .code()
        .unwrap_or_else(|| 128 + status.signal().unwrap_or(1)))
}
