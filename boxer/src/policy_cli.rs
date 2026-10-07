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
        Some("init") => init(&args[2..]),
        Some("guide") if args.len() == 2 => guide(),
        Some("schema") if args.len() == 2 => schema(),
        Some("runtime-groups") if args.len() == 2 || args.len() == 3 => {
            let groups = match args.get(2) {
                Some(name) => vec![crate::policy::RuntimeGroup::parse(
                    name.to_str()
                        .ok_or_else(|| io::Error::other("Runtime group name must be UTF-8"))?,
                )?],
                None => crate::policy::RuntimeGroup::all().to_vec(),
            };
            let groups = groups
                .into_iter()
                .map(|group| {
                    let paths = crate::policy::runtime_group_paths(group)?
                        .into_iter()
                        .map(|path| json!({"path":path,"exists":path.exists()}))
                        .collect::<Vec<_>>();
                    Ok(json!({"name":group.name(),"description":group.description(),"access":"read","paths":paths}))
                })
                .collect::<io::Result<Vec<_>>>()?;
            println!(
                "{}",
                serde_json::to_string_pretty(&groups).map_err(io::Error::other)?
            );
            Ok(0)
        }
        Some("profiles") if args.len() == 2 => {
            let mut profiles: Vec<_> = PROFILES
                .iter()
                .map(|(name, description)| json!({"name":name,"description":description}))
                .collect();
            profiles.extend(
                crate::profiles::custom_profiles()?
                    .into_iter()
                    .map(|name| json!({"name":name,"description":"Custom Boxer profile"})),
            );
            println!(
                "{}",
                serde_json::to_string_pretty(&profiles).map_err(io::Error::other)?
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
            let (file, workspace, raw) = parse_show_args(args)?;
            if raw {
                let policy = Policy::from_file_raw(&file)?;
                println!("{}", serde_json::to_string_pretty(&json!({"platform":std::env::consts::OS,"workspace":workspace,"resolved":false,"policy":policy})).map_err(io::Error::other)?);
            } else {
                let policy = load(&file, &workspace)?;
                println!("{}", serde_json::to_string_pretty(&json!({"platform":std::env::consts::OS,"platform_supported":supported(&policy),"workspace":workspace,"policy":policy})).map_err(io::Error::other)?);
            }
            Ok(0)
        }
        Some("diff") if args.len() >= 4 => {
            let before_path = resolve_policy_reference(Path::new(&args[2]))?;
            let after_path = resolve_policy_reference(Path::new(&args[3]))?;
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

fn guide() -> io::Result<i32> {
    println!(
        "{}\n\nMachine-readable schema:\n{}",
        POLICY_GUIDE,
        serde_json::to_string_pretty(&schema_value()).map_err(io::Error::other)?
    );
    Ok(0)
}

const POLICY_GUIDE: &str = r#"Boxer policy authoring guide

Purpose
  A policy controls the filesystem, network, environment, and resource limits
  visible to a launched process. Policies are JSON or JSONC, versioned with
  `version: 1`. `mode` is required: `unrestricted`, `workspace`, or `isolated`.
  Validate and inspect a policy before using it to start an agent.

Start and inspect
  `boxer policy init [NAME] [--extends BASE ...] [--full] [--output FILE]`
  creates a starter policy and never overwrites an existing file. `--extends`
  may be repeated; parents merge from left to right, then the child is applied.
  List fields are combined without duplicates, nested objects merge, and later
  scalar values override earlier ones. Missing parents and malformed policies
  are rejected before the child is written.

  `boxer policy validate FILE --cwd WORKSPACE` resolves paths and checks policy
  combinations. `boxer policy show FILE --cwd WORKSPACE` prints resolved policy
  values. Add `--raw` to show the merged values before path resolution.
  `boxer policy diff BEFORE AFTER --cwd WORKSPACE` compares resolved fields.
  `boxer policy schema` prints the machine-readable schema; this guide appends
  the same schema below.

Filesystem
  `read` grants existing files or directories read access. `write` grants
  read/write access. `write_only` grants write access without reading and is
  available in workspace mode on Linux and macOS. `read_only: true` denies
  writes, so it cannot be combined with writable grants. In workspace mode,
  filesystem access starts at the workspace; add grants only when needed.
  `deny` blocks an existing file or directory even if another grant includes
  it. Denies accumulate through inheritance. macOS supports them with Seatbelt;
  Linux requires isolated mode; Windows does not yet enforce filesystem rules.
  Relative grants resolve against the file that declares them. `$HOME` and
  `$WORKSPACE` are the only supported path variables. Grant paths must exist.

Network
  `network` is `allow`, `deny`, or `proxy`. `allow` is the default for an
  initialized standalone policy. `deny` blocks socket networking. `proxy` is
  supported only in Linux isolated mode and routes HTTP(S) through Boxer's
  broker. `hosts` allows destination hosts; `deny_hosts` blocks them before
  allow rules. `network_profile` selects a built-in host set. `upstream_proxy`
  and `upstream_bypass` configure the proxy's upstream. `local` forwards a host
  service into the sandbox and `publish` exposes sandbox ports on the host.
  `proxy_port` selects the broker port. Proxy-specific fields require proxy
  mode and applicable isolated networking.

Credentials and endpoints
  `credentials` selects built-in credential routes. `custom_credentials` maps
  route names to HTTPS upstreams and injection settings. `credential_capture`
  maps `cmd://NAME` sources to trusted host commands. `endpoint_rules`
  restricts a route by provider, HTTP method, and path; each provider must also
  appear in `credentials`. Never put secret values in a policy file.

Environment
  `clean_env: true` forwards only Boxer-approved variables; `pass_env` adds
  names to that baseline. `environment.allow_vars` and `deny_vars` filter names
  (patterns support `*`); `case_insensitive_vars` changes matching behavior;
  `set_vars` adds literal values. `env_credentials` forwards named credentials
  through Boxer, while `env_credential_map` maps host names to child names.
  When extending a policy, omit scalar settings you want to inherit. Empty
  additive lists and maps do not remove values from a parent.

Runtime and resource limits
  `runtime_groups` grants read access to detected Node, Python, Rust, or Go
  toolchain files. Run `boxer policy runtime-groups [NAME]` to inspect candidate
  paths and whether each exists on this machine. This is a read-grant
  explanation, not a broader security group. Runtime groups require workspace
  or isolated mode. `cpus`,
  `memory_mib`, and `pids` limit an isolated process tree; `cgroup_root` selects
  its delegated Linux cgroup parent. Resource limits require isolated mode.

Important boundaries
  Policies describe requested rules, but available enforcement depends on the
  host OS. `policy validate` and `policy show` report current-platform support;
  they do not apply kernel controls. `boxer --policy FILE --cwd PATH --check`
  probes enforcement before launch. Unsupported restrictions fail closed.
  Do not infer that a policy is enforced merely because it parses successfully.
"#;

fn schema() -> io::Result<i32> {
    println!(
        "{}",
        serde_json::to_string_pretty(&schema_value()).map_err(io::Error::other)?
    );
    Ok(0)
}

fn schema_value() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://solmu.dev/schemas/boxer-policy-v1.json",
        "title": "Solmu Boxer policy",
        "type": "object",
        "required": ["version"],
        "additionalProperties": false,
        "properties": {
            "version": {"const": 1},
            "extends": {"oneOf": [
                {"type": "string", "minLength": 1},
                {"type": "array", "items": {"type": "string", "minLength": 1}}
            ]},
            "mode": {"enum": ["unrestricted", "workspace", "isolated"]},
            "network": {"enum": ["allow", "deny", "proxy"]},
            "supervised": {"type": "boolean"},
            "network_profile": {"enum": ["minimal", "developer", "claude-code", "codex", "opencode", "enterprise"]},
            "upstream_proxy": {"type": "string", "format": "uri"},
            "upstream_bypass": {"type": "array", "uniqueItems": true, "items": {"type": "string"}},
            "hosts": {"type": "array", "items": {"type": "string"}},
            "deny_hosts": {"type": "array", "uniqueItems": true, "items": {"type": "string"}},
            "local": {"type": "array", "items": {"type": "string"}},
            "publish": {"type": "array", "items": {"type": "integer", "minimum": 1, "maximum": 65535}},
            "proxy_port": {"type": "integer", "minimum": 1, "maximum": 65535},
            "read_only": {"type": "boolean"},
            "protect_unlink": {"type": "boolean"},
            "read": {"type": "array", "items": {"type": "string"}},
            "write": {"type": "array", "items": {"type": "string"}},
            "deny": {"type": "array", "items": {"type": "string"}},
            "write_only": {"type": "array", "items": {"type": "string"}},
            "clean_env": {"type": "boolean"},
            "pass_env": {"type": "array", "items": {"type": "string"}},
            "environment": {"type": "object", "additionalProperties": false, "properties": {
                "allow_vars": {"type": "array", "items": {"type": "string", "minLength": 1}},
                "deny_vars": {"type": "array", "items": {"type": "string", "minLength": 1}},
                "case_insensitive_vars": {"type": "boolean"},
                "set_vars": {"type": "object", "propertyNames": {"pattern": "^[A-Za-z_][A-Za-z0-9_]*$"}, "additionalProperties": {"type": "string"}}
            }},
            "env_credentials": {"type": "array", "items": {"type": "string", "pattern": "^[A-Za-z_][A-Za-z0-9_]*$"}},
            "env_credential_map": {"type": "object", "additionalProperties": {"type": "string", "pattern": "^[A-Za-z_][A-Za-z0-9_]*$"}},
            "credential_capture": {"type": "object", "propertyNames": {"pattern": "^[a-z][a-z0-9_]{0,63}$"}, "additionalProperties": {"type": "object", "additionalProperties": false, "required": ["command"], "properties": {"command": {"type": "array", "minItems": 1, "maxItems": 64, "items": {"type": "string", "minLength": 1, "maxLength": 4096}}, "timeout_secs": {"type": "integer", "minimum": 1, "maximum": 60}}}},
            "runtime_groups": {"type": "array", "uniqueItems": true, "items": {"enum": ["node", "python", "rust", "go"]}},
            "credentials": {"type": "array", "uniqueItems": true, "items": {"type": "string", "pattern": "^[a-z][a-z0-9_]{0,63}$"}},
            "custom_credentials": {"type": "object", "propertyNames": {"pattern": "^[a-z][a-z0-9_]{0,63}$"}, "additionalProperties": {"type": "object", "additionalProperties": false, "required": ["upstream", "credential_key"], "properties": {"upstream": {"type": "string", "format": "uri"}, "credential_key": {"type": "string"}, "env_var": {"type": "string", "pattern": "^[A-Za-z_][A-Za-z0-9_]*$"}, "inject_mode": {"enum": ["header", "url_path", "query_param", "basic_auth"]}, "inject_header": {"type": "string"}, "credential_format": {"type": "string"}, "path_pattern": {"type": "string"}, "path_replacement": {"type": "string"}, "query_param_name": {"type": "string"}}}},
            "endpoint_rules": {"type": "array", "uniqueItems": true, "items": {"type": "object", "additionalProperties": false, "required": ["provider", "method", "path"], "properties": {"provider": {"type": "string", "pattern": "^[a-z][a-z0-9_]{0,63}$"}, "method": {"type": "string", "pattern": "^(\\*|[A-Z][A-Z0-9!#$&'+.^_`|~-]*)$"}, "path": {"type": "string"}}}},
            "cpus": {"type": "integer", "minimum": 1},
            "memory_mib": {"type": "integer", "minimum": 1},
            "pids": {"type": "integer", "minimum": 1},
            "linux_signal_scope": {"type": "boolean"},
            "linux_abstract_unix_socket_scope": {"type": "boolean"},
            "cgroup_root": {"type": "string"}
        },
        "allOf": [{
            "if": {"not": {"required": ["extends"]}},
            "then": {"required": ["mode"]}
        }, {
            "if": {"properties": {"network": {"const": "proxy"}}, "required": ["network"]},
            "then": {"properties": {"mode": {"const": "isolated"}}}
        }]
    })
}

fn init(args: &[OsString]) -> io::Result<i32> {
    let mut name = None;
    let mut output = None;
    let mut extends = Vec::new();
    let mut full = false;
    let mut index = 0;
    while index < args.len() {
        if args[index] == "--output" && index + 1 < args.len() {
            if output.is_some() {
                return Err(usage());
            }
            output = Some(PathBuf::from(&args[index + 1]));
            index += 2;
        } else if args[index] == "--extends" && index + 1 < args.len() {
            extends.push(args[index + 1].clone());
            index += 2;
        } else if args[index] == "--full" && !full {
            full = true;
            index += 1;
        } else if name.is_none()
            && let Some(value) = args[index].to_str()
            && valid_profile_name(value)
        {
            name = Some(value.to_owned());
            index += 1;
        } else {
            return Err(usage());
        }
    }
    let output = match output {
        Some(output) => output,
        None if name.is_some() => {
            let directory = crate::profiles::profile_directory()?;
            std::fs::create_dir_all(&directory)?;
            directory.join(format!("{}.json", name.as_deref().expect("name is set")))
        }
        None => PathBuf::from("boxer-policy.json"),
    };
    for parent in &extends {
        let parent = parent
            .to_str()
            .ok_or_else(|| io::Error::other("--extends profile must be valid UTF-8"))?;
        Policy::validate_parent(&output, parent).map_err(|error| {
            io::Error::other(format!("Cannot extend policy from {parent}: {error}"))
        })?;
    }
    let policy = scaffold(extends, full);
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&output)
        .map_err(|error| {
            if error.kind() == io::ErrorKind::AlreadyExists {
                io::Error::other(format!(
                    "Refusing to overwrite existing policy {}; choose another --output path",
                    output.display()
                ))
            } else {
                error
            }
        })?;
    serde_json::to_writer_pretty(&mut file, &policy).map_err(io::Error::other)?;
    use std::io::Write;
    file.write_all(b"\n")?;
    file.sync_all()?;
    println!("Created policy: {}", output.display());
    Ok(0)
}

fn scaffold(extends: Vec<OsString>, full: bool) -> Value {
    let inherited = !extends.is_empty();
    // `OsString` serializes as a platform-specific tagged value on Windows and
    // Unix. Profile references are validated UTF-8 strings, so keep the policy
    // format portable by serializing their textual form explicitly.
    let extends = extends
        .into_iter()
        .map(|value| value.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    let mut policy = if extends.len() == 1 {
        json!({"version":1,"extends":extends[0],"read":[],"write":[]})
    } else if !extends.is_empty() {
        json!({"version":1,"extends":extends,"read":[],"write":[]})
    } else {
        json!({"version":1,"mode":"workspace","network":"allow","read":[],"write":[]})
    };
    if full {
        let object = policy
            .as_object_mut()
            .expect("policy scaffold is an object");
        for (key, value) in [
            ("upstream_bypass", json!([])),
            ("hosts", json!([])),
            ("deny_hosts", json!([])),
            ("local", json!([])),
            ("publish", json!([])),
            ("deny", json!([])),
            ("write_only", json!([])),
            ("pass_env", json!([])),
            // `allow_vars` and `case_insensitive_vars` affect inherited behavior;
            // keep them absent so a full scaffold remains a neutral extension.
            ("environment", json!({"deny_vars":[],"set_vars":{}})),
            ("env_credentials", json!([])),
            ("env_credential_map", json!({})),
            ("runtime_groups", json!([])),
            ("credentials", json!([])),
            ("custom_credentials", json!({})),
            ("credential_capture", json!({})),
            ("linux_signal_scope", json!(false)),
            ("linux_abstract_unix_socket_scope", json!(false)),
            ("endpoint_rules", json!([])),
        ] {
            object.insert(key.into(), value);
        }
        if !inherited {
            // Make default scalar values visible only for a standalone policy.
            // In an extending policy even `false` would override its parent.
            object.insert("read_only".into(), json!(false));
            object.insert("protect_unlink".into(), json!(false));
            object.insert("clean_env".into(), json!(false));
            object.insert(
                "environment".into(),
                json!({"deny_vars":[],"case_insensitive_vars":false,"set_vars":{}}),
            );
        }
    }
    policy
}

fn valid_profile_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

fn parse_file_and_cwd(args: &[OsString], file_index: usize) -> io::Result<(PathBuf, PathBuf)> {
    let file = resolve_policy_reference(Path::new(&args[file_index]))?;
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

fn parse_show_args(args: &[OsString]) -> io::Result<(PathBuf, PathBuf, bool)> {
    let file = resolve_policy_reference(Path::new(&args[2]))?;
    let mut workspace = std::env::current_dir()?;
    let mut raw = false;
    let mut index = 3;
    while index < args.len() {
        if args[index] == "--cwd" && index + 1 < args.len() {
            workspace = PathBuf::from(&args[index + 1]);
            index += 2;
        } else if args[index] == "--raw" && !raw {
            raw = true;
            index += 1;
        } else {
            return Err(usage());
        }
    }
    Ok((file, workspace.canonicalize()?, raw))
}

fn resolve_policy_reference(reference: &Path) -> io::Result<PathBuf> {
    if reference.is_file() || reference.components().count() != 1 {
        return Ok(reference.to_owned());
    }
    let Some(name) = reference.to_str().filter(|name| valid_profile_name(name)) else {
        return Ok(reference.to_owned());
    };
    let directory = crate::profiles::profile_directory()?;
    let jsonc = directory.join(format!("{name}.jsonc"));
    if jsonc.is_file() {
        Ok(jsonc)
    } else {
        Ok(directory.join(format!("{name}.json")))
    }
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
        "Usage: boxer policy guide | init [NAME] [--extends PROFILE [--extends PROFILE ...]] [--full] [--output FILE] | schema | profiles | runtime-groups [NAME] | validate FILE [--cwd PATH] | show FILE [--cwd PATH] | diff BEFORE AFTER [--cwd PATH]",
    )
}
