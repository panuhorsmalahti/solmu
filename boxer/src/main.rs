#![recursion_limit = "256"]

use std::{ffi::OsString, io, process::Command};
mod check;
mod credential;
mod explain;
#[cfg(target_os = "linux")]
mod learn;
mod network;
mod policy;
mod policy_cli;
mod profiles;
mod rollback;
#[cfg(unix)]
mod sessions;
mod trust;
use policy::{AgentProfile, EndpointRule, Mode, Network, Policy, RuntimeGroup};
use profiles::Profile;
#[cfg(any(target_os = "linux", target_os = "macos"))]
mod unix;

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
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    let session_child: Option<String> = {
        #[cfg(unix)]
        {
            sessions::child_id(&arguments)
        }
        #[cfg(not(unix))]
        {
            None
        }
    };
    #[cfg(unix)]
    let session_daemon = sessions::daemon_id(&arguments);
    #[cfg(not(unix))]
    let session_daemon: Option<String> = None;
    #[cfg(unix)]
    sessions::set_child(session_child.is_some());
    #[cfg(unix)]
    if let Some(id) = &session_child
        && let Err(error) = sessions::await_registered(id)
    {
        eprintln!("Solmu Boxer: {error}");
        std::process::exit(125);
    }
    let result = if session_daemon.is_some() {
        #[cfg(unix)]
        {
            sessions::daemon(&arguments)
        }
        #[cfg(not(unix))]
        {
            Err(io::Error::other(
                "Detached sessions are currently supported on Linux and macOS",
            ))
        }
    } else if arguments.first().is_some_and(|arg| {
        [
            "sessions", "ps", "attach", "detach", "pause", "resume", "inspect", "stop", "prune",
        ]
        .iter()
        .any(|command| arg == command)
    }) {
        #[cfg(unix)]
        {
            sessions::command(&arguments)
        }
        #[cfg(not(unix))]
        {
            Err(io::Error::other(
                "Detached sessions are currently supported on Linux and macOS",
            ))
        }
    } else {
        run()
    };
    let result = if session_child.is_some() {
        let code = result.unwrap_or_else(|error| {
            eprintln!("Solmu Boxer: {error}");
            125
        });
        #[cfg(unix)]
        if let Some(id) = session_child.as_deref()
            && let Err(error) = sessions::finish(id, code)
        {
            eprintln!("Solmu Boxer: could not record session completion: {error}");
        }
        Ok(code)
    } else {
        result
    };
    let code = match result {
        Ok(code) => code,
        Err(error) => {
            eprintln!("Solmu Boxer: {error}");
            125
        }
    };
    std::process::exit(code);
}

fn prepare_agent_home(policy: &mut Policy, agent: AgentProfile) -> io::Result<()> {
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .ok_or_else(|| io::Error::other("Cannot find the user home directory"))?;
    let state = std::path::PathBuf::from(home)
        .join(".boxer")
        .join("profiles")
        .join(agent.name());
    let config = match agent {
        AgentProfile::Codex => vec![".codex"],
        AgentProfile::ClaudeCode => vec![".claude"],
        AgentProfile::OpenCode => vec!["config", "data", "cache", "log", "state"],
        AgentProfile::Pi => vec!["agent"],
    };
    for directory in config {
        std::fs::create_dir_all(state.join(directory))?;
    }
    std::fs::create_dir_all(state.join("tmp"))?;
    let state = state.canonicalize()?;
    policy.write.push(state.clone());
    policy.profile_home = Some(state);
    Ok(())
}

fn run() -> io::Result<i32> {
    let mut raw_arguments: Vec<OsString> = std::env::args_os().skip(1).collect();
    #[cfg(unix)]
    if raw_arguments
        .first()
        .is_some_and(|arg| arg == sessions::CHILD_ARGUMENT)
    {
        raw_arguments.drain(..2);
    }
    if raw_arguments.first().is_some_and(|arg| arg == "run") {
        raw_arguments.remove(0);
    }
    if raw_arguments.first().is_some_and(|arg| arg == "learn") {
        #[cfg(target_os = "linux")]
        return learn::command(&raw_arguments);
        #[cfg(not(target_os = "linux"))]
        return Err(io::Error::other(
            "Boxer learn is currently supported on Linux and requires strace",
        ));
    }
    if raw_arguments
        .first()
        .is_some_and(|argument| argument == "credential")
    {
        return credential::command(&raw_arguments);
    }
    if raw_arguments
        .first()
        .is_some_and(|argument| argument == "trust")
    {
        return trust::command(&raw_arguments);
    }
    if raw_arguments
        .first()
        .is_some_and(|argument| argument == "policy")
    {
        return policy_cli::command(&raw_arguments);
    }
    if raw_arguments
        .first()
        .is_some_and(|argument| argument == "network")
    {
        return network::command(&raw_arguments);
    }
    let why_command = raw_arguments
        .first()
        .is_some_and(|argument| argument == "why");
    if why_command {
        raw_arguments.remove(0);
    }
    let rollback_child = raw_arguments
        .first()
        .is_some_and(|argument| argument == rollback::CHILD_ARGUMENT);
    if rollback_child {
        raw_arguments.remove(0);
    }
    if !rollback_child
        && raw_arguments
            .first()
            .is_some_and(|argument| argument == "rollback")
    {
        return rollback::command(&raw_arguments);
    }
    #[cfg(target_os = "linux")]
    if let Some(code) = linux::proxy::worker()? {
        return Ok(code);
    }
    if let Some(code) = check::worker()? {
        return Ok(code);
    }
    let mut arguments = raw_arguments.iter().cloned();
    let mut policy = Policy::default();
    let mut mode = None;
    let mut network = None;
    let mut network_profile = None;
    let mut policy_file = None;
    let mut profile: Option<Profile> = None;
    let mut print_policy = false;
    let mut check_policy = false;
    let mut rollback_session = false;
    let mut detached = false;
    let mut why_path = None;
    let mut why_host = None;
    let mut why_operation = "read";
    let mut trust_key = None;
    let mut trust_policy = None;
    let mut verify_files = Vec::new();
    let mut directory = None;
    let mut program: Option<OsString> = None;
    let mut command_arguments = Vec::new();
    while let Some(argument) = arguments.next() {
        if argument == "--help" || argument == "-h" {
            println!(
                "Solmu Boxer\n\nUsage: boxer [OPTIONS] [--] [PROGRAM [ARGS...]]\n\nDefault program: solmu\nDefault permissions: filesystem and all network requests allowed.\n--cwd PATH: project working directory.\n--workspace, --allow-cwd: Linux/macOS filesystem allowlist; writable project, read-only runtime files.\n--read PATH: additional existing read-only file or directory (repeatable).\n--write PATH: additional existing writable file or directory (repeatable).\n--write-only PATH: additional existing write-only file or directory (repeatable; Linux/macOS workspace mode).\n--profile solmu|codex|claude-code|opencode|pi|NAME: built-ins or a custom JSON profile from ~/.config/boxer/profiles (BOXER_PROFILE_DIR overrides it). Agent profiles use separate login homes.\n--policy FILE: explicit versioned JSON policy; never loaded implicitly.\n--print-policy: print resolved policy as JSON without starting a program.\n--clean-env: forward only basic terminal, provider, proxy, and Solmu settings.\n--pass-env NAME: preserve an additional environment variable (repeatable).\n--read-only: deny filesystem writes on Linux/macOS.\n--isolated: Linux namespaces, seccomp, cgroups, and dropped capabilities. Requires Bubblewrap and delegated cgroup v2. Network remains allowed by default.\n--cpus N: isolated CPU quota in cores (default 2).\n--memory-mib N: isolated memory limit (default 2048 MiB, no swap).\n--pids N: isolated process/thread limit (default 256).\n--cgroup-root PATH: delegated cgroup parent (or SOLMU_CGROUP_ROOT; auto-detects systemd delegation).\nWindows: kernel Job Object contains the process tree; filesystem and network restrictions are rejected."
            );
            println!(
                "--network allow|deny|proxy: unrestricted (default), offline on Linux/macOS, or a routed Linux isolated network.\n--allow-host DOMAIN[:PORT]: exact or wildcard remote hostname for proxy networking, default port 443 (repeatable).\n--deny-host DOMAIN: deny a domain even when another rule allows it (repeatable; * matches all and * may replace complete labels).\n--allow-local IP:PORT or --open-port PORT: forward a host loopback service into the private network (repeatable).\n--publish PORT or --listen-port PORT: expose a guest service on the same host loopback port (repeatable).\n--proxy-port PORT: use a fixed local port for the Linux network proxy (otherwise an available port is chosen)."
            );
            println!(
                "--check: test enforcement in a short-lived Boxer process without starting the requested program."
            );
            println!(
                "--allow-env PATTERN / --deny-env PATTERN: add inherited environment filter patterns (repeatable; * matches any run of characters). See the Boxer guide."
            );
            println!(
                "boxer why --path PATH [--op read|write] or --host HOST[:PORT] [--op connect] [policy options]: explain resolved filesystem or network policy without launching a program."
            );
            println!(
                "boxer learn [--json] [--timeout SECONDS] [--policy FILE | --profile NAME] -- PROGRAM [ARGS...]: trace Linux access and optionally compare it with a policy (requires strace)."
            );
            println!(
                "--trust-key PUBLIC_KEY --verify FILE: verify signed files before launch; repeat --verify for multiple files."
            );
            println!(
                "--trust-key PUBLIC_KEY --trust-policy FILE: verify a signed list of files before launch."
            );
            println!(
                "--network-profile minimal|developer|claude-code|codex|opencode|enterprise: add a built-in domain allowlist to --isolated --network proxy."
            );
            println!(
                "--runtime-group node|python|rust|go: read access to detected toolchain files (repeatable; requires --workspace or --isolated)."
            );
            println!(
                "--credential PROVIDER: proxy a key from the OS credential store so the agent receives only a per-session token (built-ins: openai, anthropic, gemini, github, gitlab; custom routes come from the policy file). Requires --isolated --network proxy."
            );
            println!(
                "--allow-endpoint PROVIDER:METHOD:PATH: allow a brokered API endpoint (repeatable; * matches one path segment and ** matches multiple)."
            );
            println!(
                "--upstream-proxy http://HOST[:PORT]: route remote connections through an HTTP CONNECT proxy. --upstream-bypass DOMAIN skips it for an exact domain or *.DOMAIN pattern."
            );
            println!(
                "boxer trust keygen|sign|verify: create an Ed25519 key, sign a file, or verify its signature."
            );
            println!(
                "boxer credential set|status|delete NAME: manage credentials in the OS credential store; --env-credential NAME or --env-credential-map SOURCE TARGET loads a value into a program's environment."
            );
            println!(
                "--rollback: snapshot the workspace before and after a command. Use `boxer rollback list|show|restore|cleanup` to review, restore, and prune snapshots; `boxer rollback audit list|show|verify` reviews the local audit trail."
            );
            println!(
                "--detached: start a background terminal session; use `boxer attach <id>` and Ctrl-] then d to detach by default (Linux/macOS). Set BOXER_DETACH_SEQUENCE to configure the key sequence. Manage sessions with `boxer ps|inspect|pause|resume|stop|prune`."
            );
            return Ok(0);
        } else if argument == "--version" {
            println!("Solmu Boxer {}", env!("CARGO_PKG_VERSION"));
            return Ok(0);
        } else if argument == "--read-only" {
            policy.read_only = true;
        } else if argument == "--workspace" || argument == "--allow-cwd" {
            mode = Some(Mode::Workspace);
        } else if argument == "--network" {
            network = Some(match arguments.next().as_deref() {
                Some(value) if value == "allow" => Network::Allow,
                Some(value) if value == "deny" => Network::Deny,
                Some(value) if value == "proxy" => Network::Proxy,
                _ => return Err(io::Error::other("--network requires allow, deny, or proxy")),
            });
        } else if argument == "--network-profile" {
            if network_profile.is_some() {
                return Err(io::Error::other("Specify only one --network-profile"));
            }
            network_profile = Some(
                arguments
                    .next()
                    .and_then(|value| value.into_string().ok())
                    .ok_or_else(|| io::Error::other("--network-profile requires a profile name"))?,
            );
        } else if argument == "--upstream-proxy" {
            if policy.upstream_proxy.is_some() {
                return Err(io::Error::other("Specify only one --upstream-proxy"));
            }
            policy.upstream_proxy = Some(zeroize::Zeroizing::new(
                arguments
                    .next()
                    .and_then(|value| value.into_string().ok())
                    .ok_or_else(|| {
                        io::Error::other("--upstream-proxy requires an HTTP proxy URL")
                    })?,
            ));
        } else if argument == "--upstream-bypass" {
            policy.upstream_bypass.push(
                arguments
                    .next()
                    .and_then(|value| value.into_string().ok())
                    .ok_or_else(|| {
                        io::Error::other("--upstream-bypass requires a domain pattern")
                    })?,
            );
        } else if argument == "--allow-host" || argument == "--allow-local" {
            let value = arguments
                .next()
                .and_then(|value| value.into_string().ok())
                .ok_or_else(|| {
                    io::Error::other("Network routes require a host and optional port")
                })?;
            if argument == "--allow-host" {
                policy.hosts.push(value);
            } else {
                policy.local.push(value);
            }
        } else if argument == "--open-port" {
            let port = arguments
                .next()
                .and_then(|value| value.to_str().and_then(|value| value.parse::<u16>().ok()))
                .filter(|port| *port != 0)
                .ok_or_else(|| io::Error::other("--open-port requires a port from 1 to 65535"))?;
            policy.local.push(format!("127.0.0.1:{port}"));
        } else if argument == "--deny-host" {
            policy.deny_hosts.push(
                arguments
                    .next()
                    .and_then(|value| value.into_string().ok())
                    .ok_or_else(|| io::Error::other("--deny-host requires a domain pattern"))?,
            );
        } else if argument == "--publish" || argument == "--listen-port" {
            policy.publish.push(
                arguments
                    .next()
                    .and_then(|value| value.to_str().and_then(|value| value.parse::<u16>().ok()))
                    .filter(|port| *port != 0)
                    .ok_or_else(|| {
                        io::Error::other("--publish/--listen-port requires a port from 1 to 65535")
                    })?,
            );
        } else if argument == "--proxy-port" {
            policy.proxy_port = Some(
                arguments
                    .next()
                    .and_then(|value| value.to_str().and_then(|value| value.parse::<u16>().ok()))
                    .filter(|port| *port != 0)
                    .ok_or_else(|| {
                        io::Error::other("--proxy-port requires a port from 1 to 65535")
                    })?,
            );
        } else if argument == "--isolated" {
            mode = Some(Mode::Isolated);
        } else if argument == "--read" || argument == "--write" || argument == "--write-only" {
            let path = arguments
                .next()
                .ok_or_else(|| {
                    io::Error::other("--read, --write, and --write-only require a path")
                })?
                .into();
            if argument == "--read" {
                policy.read.push(path);
            } else if argument == "--write-only" {
                policy.write_only.push(path);
            } else {
                policy.write.push(path);
            }
        } else if argument == "--clean-env" {
            policy.clean_env = true;
        } else if argument == "--allow-env" || argument == "--deny-env" {
            let value = arguments
                .next()
                .and_then(|value| value.into_string().ok())
                .ok_or_else(|| {
                    io::Error::other(format!("{} requires a pattern", argument.to_string_lossy()))
                })?;
            let environment = policy.environment.get_or_insert_with(Default::default);
            if argument == "--allow-env" {
                environment
                    .allow_vars
                    .get_or_insert_with(Vec::new)
                    .push(value);
            } else {
                environment.deny_vars.push(value);
            }
        } else if argument == "--pass-env" {
            policy.pass_env.push(
                arguments
                    .next()
                    .and_then(|value| value.into_string().ok())
                    .ok_or_else(|| io::Error::other("--pass-env requires a variable name"))?,
            );
        } else if argument == "--env-credential" {
            policy.env_credentials.push(
                arguments
                    .next()
                    .and_then(|value| value.into_string().ok())
                    .ok_or_else(|| io::Error::other("--env-credential requires a variable name"))?,
            );
        } else if argument == "--env-credential-map" {
            let source = arguments
                .next()
                .and_then(|value| value.into_string().ok())
                .ok_or_else(|| io::Error::other("--env-credential-map requires a source"))?;
            let target = arguments
                .next()
                .and_then(|value| value.into_string().ok())
                .ok_or_else(|| {
                    io::Error::other("--env-credential-map requires a target environment variable")
                })?;
            policy.env_credential_map.insert(source, target);
        } else if argument == "--runtime-group" {
            policy.runtime_groups.push(RuntimeGroup::parse(
                &arguments
                    .next()
                    .and_then(|value| value.into_string().ok())
                    .ok_or_else(|| io::Error::other("--runtime-group requires a group name"))?,
            )?);
        } else if argument == "--credential" {
            policy.credentials.push(
                arguments
                    .next()
                    .and_then(|value| value.into_string().ok())
                    .ok_or_else(|| io::Error::other("--credential requires a provider name"))?,
            );
        } else if argument == "--allow-endpoint" {
            policy.endpoint_rules.push(EndpointRule::parse(
                &arguments
                    .next()
                    .and_then(|value| value.into_string().ok())
                    .ok_or_else(|| {
                        io::Error::other("--allow-endpoint requires PROVIDER:METHOD:PATH")
                    })?,
            )?);
        } else if argument == "--print-policy" {
            print_policy = true;
        } else if argument == "--check" {
            check_policy = true;
        } else if argument == "--rollback" {
            rollback_session = true;
        } else if argument == "--detached" {
            detached = true;
        } else if argument == "--path" && why_command {
            why_path =
                Some(std::path::PathBuf::from(arguments.next().ok_or_else(
                    || io::Error::other("why requires --path PATH"),
                )?));
        } else if argument == "--host" && why_command {
            why_host = Some(
                arguments
                    .next()
                    .and_then(|value| value.into_string().ok())
                    .ok_or_else(|| io::Error::other("why requires --host HOST[:PORT]"))?,
            );
        } else if argument == "--op" && why_command {
            why_operation = match arguments
                .next()
                .and_then(|value| value.into_string().ok())
                .as_deref()
            {
                Some("read") => "read",
                Some("write") => "write",
                Some("connect") => "connect",
                _ => return Err(io::Error::other("--op must be read, write, or connect")),
            };
        } else if argument == "--trust-key" {
            if trust_key.is_some() {
                return Err(io::Error::other("Specify only one --trust-key"));
            }
            trust_key = Some(std::path::PathBuf::from(arguments.next().ok_or_else(
                || io::Error::other("--trust-key requires a public key file"),
            )?));
        } else if argument == "--verify" {
            verify_files.push(std::path::PathBuf::from(
                arguments
                    .next()
                    .ok_or_else(|| io::Error::other("--verify requires a file"))?,
            ));
        } else if argument == "--trust-policy" {
            if trust_policy.is_some() {
                return Err(io::Error::other("Specify only one --trust-policy"));
            }
            trust_policy =
                Some(std::path::PathBuf::from(arguments.next().ok_or_else(
                    || io::Error::other("--trust-policy requires a file"),
                )?));
        } else if argument == "--policy" {
            if policy_file.is_some() {
                return Err(io::Error::other("Specify only one --policy file"));
            }
            policy_file =
                Some(std::path::PathBuf::from(arguments.next().ok_or_else(
                    || io::Error::other("--policy requires a path"),
                )?));
        } else if argument == "--profile" {
            if profile.is_some() {
                return Err(io::Error::other("Specify only one --profile"));
            }
            let name = arguments
                .next()
                .ok_or_else(|| io::Error::other("--profile requires a profile name"))?;
            profile = Some(Profile::parse(&name)?);
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
    if profile.is_some() && policy_file.is_some() {
        return Err(io::Error::other("Choose either --profile or --policy"));
    }
    let mut resolved = if let Some(path) = policy_file {
        Policy::from_file(&path)?
    } else if let Some(profile) = profile.as_ref() {
        if let Some(path) = profile.custom_policy() {
            Policy::from_file(path)?
        } else {
            Policy {
                mode: Mode::Workspace,
                clean_env: true,
                solmu: profile.is_solmu(),
                agent: profile.agent(),
                ..Policy::default()
            }
        }
    } else {
        Policy::default()
    };
    resolved.mode = mode.unwrap_or(resolved.mode);
    resolved.network = network.unwrap_or(resolved.network);
    resolved.network_profile = network_profile.or(resolved.network_profile);
    resolved.upstream_proxy = policy.upstream_proxy.or(resolved.upstream_proxy);
    resolved.upstream_bypass.extend(policy.upstream_bypass);
    resolved.hosts.extend(policy.hosts);
    resolved.deny_hosts.extend(policy.deny_hosts);
    resolved.local.extend(policy.local);
    resolved.publish.extend(policy.publish);
    resolved.proxy_port = policy.proxy_port.or(resolved.proxy_port);
    resolved.read_only |= policy.read_only;
    resolved.clean_env |= policy.clean_env;
    if let Some(cli_environment) = policy.environment.take() {
        let environment = resolved.environment.get_or_insert_with(Default::default);
        if let Some(allow_vars) = cli_environment.allow_vars {
            environment
                .allow_vars
                .get_or_insert_with(Vec::new)
                .extend(allow_vars);
        }
        environment.deny_vars.extend(cli_environment.deny_vars);
        environment.case_insensitive_vars |= cli_environment.case_insensitive_vars;
    }
    resolved.read.extend(policy.read);
    resolved.write.extend(policy.write);
    resolved.write_only.extend(policy.write_only);
    resolved.pass_env.extend(policy.pass_env);
    resolved.env_credentials.extend(policy.env_credentials);
    resolved
        .env_credential_map
        .extend(policy.env_credential_map);
    resolved.runtime_groups.extend(policy.runtime_groups);
    resolved.credentials.extend(policy.credentials);
    resolved
        .custom_credentials
        .extend(policy.custom_credentials);
    resolved.endpoint_rules.extend(policy.endpoint_rules);
    resolved.cpus = policy.cpus.or(resolved.cpus);
    resolved.memory_mib = policy.memory_mib.or(resolved.memory_mib);
    resolved.pids = policy.pids.or(resolved.pids);
    resolved.cgroup_root = policy.cgroup_root.or(resolved.cgroup_root);
    if let Some(agent) = resolved.agent.filter(|_| !why_command) {
        prepare_agent_home(&mut resolved, agent)?;
    }
    let workspace = directory
        .map(std::path::PathBuf::from)
        .unwrap_or(std::env::current_dir()?)
        .canonicalize()?;
    if !workspace.is_dir() {
        return Err(io::Error::other("Workspace must be a directory"));
    }
    resolved.resolve(&workspace)?;
    if !verify_files.is_empty() || trust_policy.is_some() {
        if why_command || check_policy || print_policy {
            return Err(io::Error::other(
                "Signature verification requires a normal Boxer launch",
            ));
        }
        let key = trust_key.as_deref().ok_or_else(|| {
            io::Error::other("Signature verification requires --trust-key PUBLIC_KEY")
        })?;
        if trust_policy.is_some() && !verify_files.is_empty() {
            return Err(io::Error::other(
                "Choose --trust-policy or repeated --verify flags",
            ));
        }
        if let Some(policy) = trust_policy.as_deref() {
            trust::verify_policy(key, policy, &workspace)?;
        }
        let files: Vec<_> = verify_files
            .iter()
            .map(|path| {
                if path.is_absolute() {
                    path.clone()
                } else {
                    workspace.join(path)
                }
            })
            .collect();
        trust::verify_files(key, &files)?;
    } else if trust_key.is_some() {
        return Err(io::Error::other(
            "--trust-key requires --verify FILE or --trust-policy FILE",
        ));
    }
    if why_command {
        if why_path.is_some() == why_host.is_some()
            || program.is_some()
            || rollback_session
            || check_policy
            || print_policy
        {
            return Err(io::Error::other(
                "Usage: boxer why --path PATH [--op read|write] or --host HOST[:PORT] [--op connect] [policy options]",
            ));
        }
        if let Some(host) = why_host {
            if why_operation != "connect" {
                return Err(io::Error::other("Network queries require --op connect"));
            }
            return explain::run_network(&resolved, &host);
        }
        if why_operation == "connect" {
            return Err(io::Error::other(
                "Filesystem queries require --op read or --op write",
            ));
        }
        return explain::run(&resolved, &workspace, &why_path.unwrap(), why_operation);
    }
    if rollback_session && (check_policy || print_policy) {
        return Err(io::Error::other(
            "--rollback cannot be combined with --check or --print-policy",
        ));
    }
    if check_policy {
        if print_policy {
            return Err(io::Error::other("Choose either --check or --print-policy"));
        }
        return check::run(&resolved, &workspace);
    }
    let mut command = Command::new(program.unwrap_or_else(|| {
        profile
            .as_ref()
            .and_then(Profile::program)
            .unwrap_or("solmu")
            .into()
    }));
    command.args(command_arguments).current_dir(&workspace);
    if detached {
        if rollback_session {
            return Err(io::Error::other(
                "--detached cannot be combined with --rollback",
            ));
        }
        #[cfg(unix)]
        {
            let forwarded: Vec<_> = raw_arguments
                .into_iter()
                .filter(|arg| arg != "--detached")
                .collect();
            return sessions::start(
                &forwarded,
                &workspace,
                &command.get_program().to_string_lossy(),
            );
        }
        #[cfg(not(unix))]
        return Err(io::Error::other(
            "Detached sessions are currently supported on Linux and macOS",
        ));
    }
    if rollback_session && !rollback_child {
        return rollback::run(&raw_arguments, &workspace, command.get_program());
    }
    resolved.environment(&mut command)?;
    if print_policy {
        let supported = if cfg!(windows) {
            resolved.mode == Mode::Unrestricted
                && !resolved.read_only
                && resolved.network == Network::Allow
        } else if cfg!(target_os = "macos") {
            !resolved.isolated && resolved.network != Network::Proxy
        } else {
            cfg!(target_os = "linux")
                && ((resolved.network == Network::Allow && !resolved.isolated)
                    || cfg!(any(target_arch = "x86_64", target_arch = "aarch64")))
        };
        let forwarded: Vec<_> = command
            .get_envs()
            .filter(|(_, value)| value.is_some())
            .map(|(name, _)| name.to_string_lossy().into_owned())
            .collect();
        println!("{}", serde_json::to_string_pretty(&serde_json::json!({
            "version": 1, "platform": std::env::consts::OS, "platform_supported": supported,
            "profile": profile.as_ref().map(Profile::name),
            "enforcement": "not-applied",
            "workspace": workspace, "policy": resolved,
            "network": match resolved.network { Network::Allow => "allowed", Network::Deny => "denied", Network::Proxy => "routed" },
            "runtime_read": match resolved.mode {
                Mode::Unrestricted => Vec::new(),
                Mode::Workspace => policy::runtime_paths(),
                Mode::Isolated => policy::ISOLATED_RUNTIME.iter().map(std::path::PathBuf::from).collect(),
            },
            "device_io": if resolved.mode == Mode::Workspace { policy::device_paths() } else { Vec::new() },
            "runtime_list": if resolved.mode == Mode::Workspace { policy::runtime_list() } else { Vec::new() },
            "private_mounts": if resolved.isolated { vec!["/proc", "/dev", "/tmp"] } else { Vec::new() },
            "environment": {"inherit": !resolved.clean_env && !resolved.isolated, "forwarded_names": forwarded},
            "program": command.get_program().to_string_lossy(), "arguments": command.get_args().map(|argument| argument.to_string_lossy()).collect::<Vec<_>>(),
            "resolved_program": policy::executable(&command).ok()
        })).map_err(io::Error::other)?);
        return Ok(0);
    }
    let credentials = credential::load(&resolved.env_credentials)?;
    for (name, value) in credentials {
        command.env(name, value.as_str());
    }
    let mapped_sources: Vec<_> = resolved.env_credential_map.keys().cloned().collect();
    let credentials = credential::load(&mapped_sources)?;
    for (source, value) in credentials {
        if let Some(target) = resolved.env_credential_map.get(&source) {
            command.env(target, value.as_str());
        }
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
