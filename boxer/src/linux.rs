use crate::{
    Policy,
    policy::{self, Mode, Network},
};
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
pub mod proxy;
mod seccomp;
pub mod unlink;

static INTERRUPTED: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);
extern "C" fn interrupted(signal: i32) {
    INTERRUPTED.store(signal, std::sync::atomic::Ordering::Relaxed);
}

pub fn run(mut command: Command, policy: Policy) -> io::Result<i32> {
    if policy.network != Network::Allow {
        crate::unix::prepare_network_denial()?;
    }
    if policy.isolated {
        return isolated(command, policy);
    }
    // The launcher is single-threaded. Apply the irreversible policy before exec,
    // rather than allocating or taking locks in a post-fork pre_exec callback.
    let handled = AccessFs::from_all(ABI::V3);
    let writable = if policy.protect_unlink {
        handled & !AccessFs::RemoveDir & !AccessFs::RemoveFile
    } else {
        handled
    };
    let allowed = if policy.read_only {
        AccessFs::from_read(ABI::V3)
    } else {
        writable
    };
    let mut rules = Ruleset::default()
        .set_compatibility(CompatLevel::HardRequirement)
        .handle_access(handled)
        .map_err(io::Error::other)?
        .create()
        .map_err(io::Error::other)?;
    if policy.mode == Mode::Workspace {
        let executable = policy::executable(&command)?;
        let mut read = policy::runtime_paths();
        read.extend(policy.read.clone());
        read.push(executable);
        let mut readable_grants = read.clone();
        readable_grants.push(
            command
                .get_current_dir()
                .expect("resolved workspace")
                .to_owned(),
        );
        readable_grants.extend(policy.write.clone());
        readable_grants.extend(policy::device_paths());
        policy.validate_write_only_overlaps(&readable_grants)?;
        for path in read {
            let access = if path.is_file() {
                AccessFs::from_read(ABI::V3) & AccessFs::from_file(ABI::V3)
            } else {
                AccessFs::from_read(ABI::V3)
            };
            rules = rules
                .add_rule(PathBeneath::new(
                    PathFd::new(path).map_err(io::Error::other)?,
                    access,
                ))
                .map_err(io::Error::other)?;
        }
        let workspace = command.get_current_dir().expect("resolved workspace");
        rules = rules
            .add_rule(PathBeneath::new(
                PathFd::new(workspace).map_err(io::Error::other)?,
                allowed,
            ))
            .map_err(io::Error::other)?;
        for path in &policy.write {
            let access = if path.is_file() {
                writable & AccessFs::from_file(ABI::V3)
            } else {
                writable
            };
            rules = rules
                .add_rule(PathBeneath::new(
                    PathFd::new(path).map_err(io::Error::other)?,
                    access,
                ))
                .map_err(io::Error::other)?;
        }
        for path in &policy.write_only {
            let write_only = AccessFs::from_write(ABI::V3);
            let write_only = if policy.protect_unlink {
                write_only & !AccessFs::RemoveDir & !AccessFs::RemoveFile
            } else {
                write_only
            };
            let access = if path.is_file() {
                write_only & AccessFs::from_file(ABI::V3)
            } else {
                write_only
            };
            rules = rules
                .add_rule(PathBeneath::new(
                    PathFd::new(path).map_err(io::Error::other)?,
                    access,
                ))
                .map_err(io::Error::other)?;
        }
        for path in policy::device_paths() {
            rules = rules
                .add_rule(PathBeneath::new(
                    PathFd::new(path).map_err(io::Error::other)?,
                    AccessFs::from_file(ABI::V3),
                ))
                .map_err(io::Error::other)?;
        }
    } else {
        rules = rules
            .add_rule(PathBeneath::new(
                PathFd::new("/").map_err(io::Error::other)?,
                allowed,
            ))
            .map_err(io::Error::other)?;
    }
    rules.restrict_self().map_err(io::Error::other)?;
    if policy.network == Network::Deny {
        seccomp::install_network_denial()?;
    }
    if crate::sessions::is_child() {
        return command.status().map(|status| {
            status
                .code()
                .unwrap_or_else(|| 128 + status.signal().unwrap_or(1))
        });
    }
    Err(command.exec())
}

fn isolated(mut command: Command, mut policy: Policy) -> io::Result<i32> {
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
    let executable = policy::executable(&command)?;
    if policy
        .deny
        .iter()
        .any(|denied| executable.as_path() == denied.as_path() || executable.starts_with(denied))
    {
        return Err(io::Error::other(
            "The isolated policy denies the executable Boxer needs to launch",
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
    ]);
    if policy.network != Network::Allow {
        sandbox.arg("--unshare-net");
    }
    // Share only runtime files, not the host's home, /run, or arbitrary mounts.
    for &path in policy::ISOLATED_RUNTIME {
        sandbox.args(["--ro-bind-try", path, path]);
    }
    sandbox.args(["--proc", "/proc", "--dev", "/dev", "--tmpfs", "/tmp"]);
    // Grants are additive, as in the native allowlists. Mount read-only grants
    // first so a read grant inside a writable tree cannot remove its writes,
    // and a read grant containing the workspace cannot hide the workspace.
    for path in &policy.read {
        sandbox.arg("--ro-bind").arg(path).arg(path);
    }
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
    for path in &policy.write {
        sandbox.arg("--bind").arg(path).arg(path);
    }
    let mut exposed_roots = policy.read.clone();
    exposed_roots.extend(policy.write.clone());
    exposed_roots.extend(policy.write_only.clone());
    exposed_roots.extend(policy::device_paths());
    exposed_roots.push(directory.clone());
    exposed_roots.extend(
        policy::ISOLATED_RUNTIME
            .iter()
            .map(|path| std::path::PathBuf::from(*path)),
    );
    for path in &policy.deny {
        if !exposed_roots
            .iter()
            .any(|root| root.starts_with(path) || path.starts_with(root))
        {
            // This path is outside every mount grant and is already absent from
            // the isolated filesystem view.
            continue;
        }
        if path.is_dir() {
            sandbox
                .arg("--tmpfs")
                .arg(path)
                .arg("--remount-ro")
                .arg(path);
        } else {
            sandbox.arg("--ro-bind").arg("/dev/null").arg(path);
        }
    }
    sandbox
        .arg("--ro-bind")
        .arg(executable)
        .arg("/opt/solmu/agent");
    let mut credential_broker = None;
    if !policy.credentials.is_empty() {
        let mut reserved_ports = policy.publish.clone();
        reserved_ports.extend(policy.proxy_port);
        reserved_ports.extend(
            policy
                .local
                .iter()
                .map(|route| crate::network::Target::parse(route, true).map(|target| target.port))
                .collect::<io::Result<Vec<_>>>()?,
        );
        let (broker, session_tokens) = proxy::credential::Broker::start(
            &policy.credentials,
            &policy.custom_credentials,
            proxy::credential::BrokerOptions {
                endpoint_rules: &policy.endpoint_rules,
                upstream_proxy: policy
                    .upstream_proxy
                    .as_deref()
                    .map(|value| crate::network::UpstreamProxy::parse(value))
                    .transpose()?
                    .as_ref(),
                upstream_bypass: &policy.upstream_bypass,
                denied_hosts: &policy.deny_hosts,
                reserved_ports: &reserved_ports,
            },
        )?;
        let port = broker.port();
        policy.local.push(format!("127.0.0.1:{port}"));
        for session in session_tokens {
            command.env(&session.token_env, session.token);
            let base = format!("http://127.0.0.1:{port}{}", session.base_path);
            if policy.solmu {
                command.env("LLM_ENDPOINT", base);
            } else {
                if session.name == "openai" {
                    command
                        .env_remove("CODEX_API_KEY")
                        .env_remove("CODEX_ACCESS_TOKEN");
                } else if session.name == "anthropic" {
                    command.env_remove("ANTHROPIC_AUTH_TOKEN");
                }
                command.env(&session.base_env, base);
            }
        }
        credential_broker = Some(broker);
    }
    // Host networking is shared only when the policy allows it.
    let group = cgroup::Group::create(&policy)?;
    group.validate_workspace(&directory)?;
    for path in policy
        .read
        .iter()
        .chain(&policy.write)
        .chain(&policy.write_only)
    {
        if path == std::path::Path::new("/")
            || ["/sys", "/proc", "/dev"]
                .iter()
                .any(|root| path.starts_with(root))
        {
            return Err(io::Error::other(
                "Isolated grants cannot expose filesystem roots or kernel control directories",
            ));
        }
        group.validate_workspace(path)?;
    }
    let filter = seccomp::filter(policy.network)?;
    let filter_fd = filter.as_raw_fd();
    let group_fd = group.as_raw_fd();
    let mut bridge = if policy.network == Network::Proxy {
        Some(proxy::Host::new(&policy)?)
    } else {
        None
    };
    let bridge_fds: Vec<_> = bridge
        .as_ref()
        .map(|bridge| bridge.children.iter().map(AsRawFd::as_raw_fd).collect())
        .unwrap_or_default();
    if let Some(bridge) = &bridge {
        let worker = proxy::Worker {
            bridge: bridge.children[0].as_raw_fd(),
            inbound: bridge.children[1].as_raw_fd(),
            arguments: command.get_args().map(std::ffi::OsStr::to_owned).collect(),
            local: policy
                .local
                .iter()
                .map(|route| crate::network::Target::parse(route, true))
                .collect::<io::Result<_>>()?,
            publish: policy.publish.clone(),
            proxy_port: policy.proxy_port,
        };
        sandbox
            .arg("--ro-bind")
            .arg(std::env::current_exe()?)
            .arg("/opt/solmu/boxer")
            .arg("--chdir")
            .arg(&directory)
            .arg("--")
            .arg("/opt/solmu/boxer")
            .arg(proxy::WORKER)
            .arg(serde_json::to_string(&worker).map_err(io::Error::other)?);
    } else {
        sandbox
            .arg("--chdir")
            .arg(&directory)
            .arg("--")
            .arg("/opt/solmu/agent")
            .args(command.get_args());
    }
    if policy.protect_unlink {
        let arguments: Vec<_> = sandbox.get_args().map(std::ffi::OsStr::to_owned).collect();
        let separator = arguments
            .iter()
            .position(|argument| argument == "--")
            .ok_or_else(|| io::Error::other("Bubblewrap command is missing its separator"))?;
        let program = arguments
            .get(separator + 1)
            .ok_or_else(|| io::Error::other("Bubblewrap command is missing its program"))?
            .clone();
        let program_arguments = arguments[separator + 2..].to_vec();
        let boxer = std::env::current_exe()?;
        let mut wrapped = Command::new("bwrap");
        wrapped.args(&arguments[..separator]);
        // Network proxy mode already mounts Boxer inside the guest.
        if policy.network != Network::Proxy {
            wrapped.arg("--ro-bind").arg(&boxer).arg("/opt/solmu/boxer");
        }
        wrapped
            .arg("--")
            .arg("/opt/solmu/boxer")
            .arg(unlink::WORKER)
            .arg(program)
            .args(program_arguments);
        sandbox = wrapped;
    }
    // Options must precede the agent separator.
    let mut supervised = Command::new("bwrap");
    supervised
        .arg("--seccomp")
        .arg(filter_fd.to_string())
        .args(sandbox.get_args());
    // Values passed as --setenv arguments appear in host process listings.
    // Give Bubblewrap the already-filtered environment directly instead.
    supervised.env_clear();
    for (key, value) in command.get_envs() {
        if let Some(value) = value {
            supervised.env(key, value);
        }
    }
    // SAFETY: callback uses only write/fcntl and errno access; descriptors stay
    // alive until the child has completed, and attachment precedes exec.
    unsafe {
        supervised.pre_exec(move || {
            cgroup::attach(group_fd)?;
            if libc::fcntl(filter_fd, libc::F_SETFD, 0) < 0 {
                return Err(io::Error::last_os_error());
            }
            for descriptor in &bridge_fds {
                if libc::fcntl(*descriptor, libc::F_SETFD, 0) < 0 {
                    return Err(io::Error::last_os_error());
                }
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
    if let Some(bridge) = &mut bridge {
        bridge.children.clear();
    }
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
    drop(credential_broker);
    Ok(status
        .code()
        .unwrap_or_else(|| 128 + status.signal().unwrap_or(1)))
}
