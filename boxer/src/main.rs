use std::{ffi::OsString, io, process::Command};
mod policy;
use policy::{Mode, Policy};

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
use linux as platform;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use macos as platform;
#[cfg(windows)]
mod windows;
#[cfg(windows)]
use windows as platform;

fn main() {
    let code = match run() {
        Ok(code) => code,
        Err(error) => {
            eprintln!("Solmu Boxer: {error}");
            125
        }
    };
    std::process::exit(code);
}

fn run() -> io::Result<i32> {
    let mut arguments = std::env::args_os().skip(1);
    let mut policy = Policy::default();
    let mut mode = None;
    let mut policy_file = None;
    let mut profile = false;
    let mut print_policy = false;
    let mut directory = None;
    let mut program: Option<OsString> = None;
    let mut command_arguments = Vec::new();
    while let Some(argument) = arguments.next() {
        if argument == "--help" || argument == "-h" {
            println!(
                "Solmu Boxer\n\nUsage: boxer [OPTIONS] [--] [PROGRAM [ARGS...]]\n\nDefault program: solmu\nDefault permissions: filesystem and all network requests allowed.\n--cwd PATH: project working directory.\n--workspace: Linux/macOS filesystem allowlist; writable project, read-only runtime files.\n--read PATH: additional existing read-only file or directory (repeatable).\n--write PATH: additional existing writable file or directory (repeatable).\n--profile solmu: workspace policy, clean environment, and Solmu workspace configuration.\n--policy FILE: explicit versioned JSON policy; never loaded implicitly.\n--print-policy: print resolved policy as JSON without starting a program.\n--clean-env: forward only basic terminal, provider, proxy, and Solmu settings.\n--pass-env NAME: preserve an additional environment variable (repeatable).\n--read-only: deny filesystem writes on Linux/macOS.\n--isolated: Linux namespaces, seccomp, cgroups, and dropped capabilities. Requires Bubblewrap and delegated cgroup v2. Network remains allowed.\n--cpus N: isolated CPU quota in cores (default 2).\n--memory-mib N: isolated memory limit (default 2048 MiB, no swap).\n--pids N: isolated process/thread limit (default 256).\n--cgroup-root PATH: delegated cgroup parent (or SOLMU_CGROUP_ROOT; auto-detects systemd delegation).\nWindows: kernel Job Object contains the process tree; filesystem restrictions are rejected."
            );
            return Ok(0);
        } else if argument == "--version" {
            println!("Solmu Boxer {}", env!("CARGO_PKG_VERSION"));
            return Ok(0);
        } else if argument == "--read-only" {
            policy.read_only = true;
        } else if argument == "--workspace" {
            mode = Some(Mode::Workspace);
        } else if argument == "--isolated" {
            mode = Some(Mode::Isolated);
        } else if argument == "--read" || argument == "--write" {
            let path = arguments
                .next()
                .ok_or_else(|| io::Error::other("--read and --write require a path"))?
                .into();
            if argument == "--read" {
                policy.read.push(path);
            } else {
                policy.write.push(path);
            }
        } else if argument == "--clean-env" {
            policy.clean_env = true;
        } else if argument == "--pass-env" {
            policy.pass_env.push(
                arguments
                    .next()
                    .and_then(|value| value.into_string().ok())
                    .ok_or_else(|| io::Error::other("--pass-env requires a variable name"))?,
            );
        } else if argument == "--print-policy" {
            print_policy = true;
        } else if argument == "--policy" {
            if policy_file.is_some() {
                return Err(io::Error::other("Specify only one --policy file"));
            }
            policy_file =
                Some(std::path::PathBuf::from(arguments.next().ok_or_else(
                    || io::Error::other("--policy requires a path"),
                )?));
        } else if argument == "--profile" {
            if arguments.next().as_deref() != Some(std::ffi::OsStr::new("solmu")) {
                return Err(io::Error::other(
                    "Unknown profile; available profile: solmu",
                ));
            }
            profile = true;
        } else if argument == "--cpus" || argument == "--memory-mib" || argument == "--pids" {
            let value = arguments
                .next()
                .and_then(|value| value.to_str().and_then(|text| text.parse::<u32>().ok()))
                .filter(|value| *value > 0)
                .ok_or_else(|| {
                    io::Error::other(format!(
                        "{} requires a positive integer",
                        argument.to_string_lossy()
                    ))
                })?;
            if argument == "--cpus" {
                policy.cpus = Some(value);
            } else if argument == "--memory-mib" {
                policy.memory_mib = Some(value);
            } else {
                policy.pids = Some(value);
            }
        } else if argument == "--cgroup-root" {
            policy.cgroup_root = Some(
                arguments
                    .next()
                    .ok_or_else(|| io::Error::other("--cgroup-root requires a path"))?
                    .into(),
            );
        } else if argument == "--cwd" {
            directory = Some(
                arguments
                    .next()
                    .ok_or_else(|| io::Error::other("--cwd requires a path"))?,
            );
        } else if argument == "--" {
            program = arguments.next();
            command_arguments.extend(arguments);
            break;
        } else if argument.to_string_lossy().starts_with('-') {
            return Err(io::Error::other(
                "Unknown option; use --help. Put program arguments after --.",
            ));
        } else {
            program = Some(argument);
            command_arguments.extend(arguments);
            break;
        }
    }
    if profile && policy_file.is_some() {
        return Err(io::Error::other("Choose either --profile or --policy"));
    }
    let mut resolved = if let Some(path) = policy_file {
        Policy::from_file(&path)?
    } else if profile {
        Policy {
            mode: Mode::Workspace,
            clean_env: true,
            solmu: true,
            ..Policy::default()
        }
    } else {
        Policy::default()
    };
    resolved.mode = mode.unwrap_or(resolved.mode);
    resolved.read_only |= policy.read_only;
    resolved.clean_env |= policy.clean_env;
    resolved.read.extend(policy.read);
    resolved.write.extend(policy.write);
    resolved.pass_env.extend(policy.pass_env);
    resolved.cpus = policy.cpus.or(resolved.cpus);
    resolved.memory_mib = policy.memory_mib.or(resolved.memory_mib);
    resolved.pids = policy.pids.or(resolved.pids);
    resolved.cgroup_root = policy.cgroup_root.or(resolved.cgroup_root);
    let workspace = directory
        .map(std::path::PathBuf::from)
        .unwrap_or(std::env::current_dir()?)
        .canonicalize()?;
    if !workspace.is_dir() {
        return Err(io::Error::other("Workspace must be a directory"));
    }
    resolved.resolve(&workspace)?;
    let mut command = Command::new(program.unwrap_or_else(|| "solmu".into()));
    command.args(command_arguments).current_dir(&workspace);
    resolved.environment(&mut command);
    if print_policy {
        let supported = if cfg!(windows) {
            resolved.mode == Mode::Unrestricted && !resolved.read_only
        } else if cfg!(target_os = "macos") {
            !resolved.isolated
        } else {
            cfg!(target_os = "linux")
        };
        let forwarded: Vec<_> = command
            .get_envs()
            .filter(|(_, value)| value.is_some())
            .map(|(name, _)| name.to_string_lossy().into_owned())
            .collect();
        println!("{}", serde_json::to_string_pretty(&serde_json::json!({
            "version": 1, "platform": std::env::consts::OS, "platform_supported": supported,
            "enforcement": "not-applied",
            "workspace": workspace, "policy": resolved, "network": "allowed",
            "runtime_read": match resolved.mode {
                Mode::Unrestricted => Vec::new(),
                Mode::Workspace => policy::runtime_paths(),
                Mode::Isolated => policy::ISOLATED_RUNTIME.iter().map(std::path::PathBuf::from).collect(),
            },
            "device_io": if resolved.mode == Mode::Workspace { policy::device_paths() } else { Vec::new() },
            "private_mounts": if resolved.isolated { vec!["/proc", "/dev", "/tmp"] } else { Vec::new() },
            "environment": {"inherit": !resolved.clean_env && !resolved.isolated, "forwarded_names": forwarded},
            "program": command.get_program().to_string_lossy(), "arguments": command.get_args().map(|argument| argument.to_string_lossy()).collect::<Vec<_>>(),
            "resolved_program": policy::executable(&command).ok()
        })).map_err(io::Error::other)?);
        return Ok(0);
    }
    #[cfg(not(target_os = "linux"))]
    if resolved.isolated {
        return Err(io::Error::other(
            "--isolated is currently supported only on Linux",
        ));
    }
    platform::run(command, resolved)
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
mod platform {
    pub fn run(_: std::process::Command, _: super::Policy) -> std::io::Result<i32> {
        Err(std::io::Error::other(
            "Kernel sandboxing is unsupported on this OS",
        ))
    }
}
