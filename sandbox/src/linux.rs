use crate::Policy;
use landlock::{
    ABI, Access, AccessFs, CompatLevel, Compatible, PathBeneath, PathFd, Ruleset, RulesetAttr,
    RulesetCreatedAttr,
};
use std::{io, os::unix::process::CommandExt, process::Command};

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
    if !directory.is_dir() || directory == std::path::Path::new("/") {
        return Err(io::Error::other(
            "--isolated requires a workspace directory other than /",
        ));
    }
    let mut sandbox = Command::new("bwrap");
    sandbox.args([
        "--unshare-user",
        "--unshare-pid",
        "--unshare-ipc",
        "--unshare-uts",
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
            "PATH" | "TERM" | "LANG" | "LC_ALL" | "COLORTERM"
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
    let error = sandbox.exec();
    Err(io::Error::other(format!(
        "Linux isolation requires Bubblewrap (bwrap) and enabled user namespaces: {error}"
    )))
}
