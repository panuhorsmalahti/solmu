use serde::{Deserialize, Serialize};
use std::{
    io::{self, Read},
    path::{Path, PathBuf},
    process::Command,
};

// Security policies must not accept ambiguous duplicate keys. Parsing into a
// Value directly would silently keep the last occurrence.
struct PolicyObject;
impl<'de> serde::de::Visitor<'de> for PolicyObject {
    type Value = serde_json::Map<String, serde_json::Value>;

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("a policy object with unique keys")
    }

    fn visit_map<A: serde::de::MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        let mut object = serde_json::Map::new();
        while let Some((key, value)) = map.next_entry::<String, serde_json::Value>()? {
            if object.insert(key.clone(), value).is_some() {
                return Err(serde::de::Error::custom(format!(
                    "Duplicate policy key: {key}"
                )));
            }
        }
        Ok(object)
    }
}

#[derive(Clone, Copy, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum Mode {
    #[default]
    Unrestricted,
    Workspace,
    Isolated,
}

#[derive(Clone, Copy, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum Network {
    #[default]
    Allow,
    Deny,
    Proxy,
}

#[derive(Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Policy {
    pub mode: Mode,
    pub network: Network,
    pub hosts: Vec<String>,
    pub local: Vec<String>,
    pub publish: Vec<u16>,
    pub read_only: bool,
    pub read: Vec<PathBuf>,
    pub write: Vec<PathBuf>,
    pub clean_env: bool,
    pub pass_env: Vec<String>,
    pub cpus: Option<u32>,
    pub memory_mib: Option<u32>,
    pub pids: Option<u32>,
    pub cgroup_root: Option<PathBuf>,
    #[serde(skip)]
    pub isolated: bool,
    #[serde(skip)]
    pub solmu: bool,
}

impl Policy {
    pub fn from_file(path: &Path) -> io::Result<Self> {
        let mut source = Vec::new();
        std::fs::File::open(path)?
            .take(1_000_001)
            .read_to_end(&mut source)?;
        if source.len() > 1_000_000 {
            return Err(io::Error::other("Policy exceeds the 1 MB limit"));
        }
        // Read the format version separately, then validate every policy field.
        let mut parser = serde_json::Deserializer::from_slice(&source);
        let mut object = serde::de::Deserializer::deserialize_map(&mut parser, PolicyObject)
            .map_err(io::Error::other)?;
        parser.end().map_err(io::Error::other)?;
        let version = object
            .remove("version")
            .and_then(|value| value.as_u64())
            .ok_or_else(|| io::Error::other("Policy requires version: 1"))?;
        if version != 1 {
            return Err(io::Error::other("Unsupported policy version; expected 1"));
        }
        if !object.contains_key("mode") {
            return Err(io::Error::other(
                "Policy requires an explicit mode: unrestricted, workspace, or isolated",
            ));
        }
        let mut policy: Self =
            serde_json::from_value(serde_json::Value::Object(object)).map_err(io::Error::other)?;
        let base = path.canonicalize()?.parent().unwrap().to_owned();
        for path in policy.read.iter_mut().chain(&mut policy.write) {
            if path.is_relative() && !path.to_string_lossy().starts_with('$') {
                *path = base.join(&*path);
            }
        }
        if let Some(root) = &mut policy.cgroup_root
            && root.is_relative()
        {
            *root = base.join(&*root);
        }
        Ok(policy)
    }

    pub fn resolve(&mut self, workspace: &Path) -> io::Result<()> {
        self.isolated = self.mode == Mode::Isolated;
        if self.network == Network::Proxy && !self.isolated {
            return Err(io::Error::other(
                "Proxy networking requires Linux --isolated mode",
            ));
        }
        if self.network != Network::Proxy
            && (!self.hosts.is_empty() || !self.local.is_empty() || !self.publish.is_empty())
        {
            return Err(io::Error::other("Network routes require --network proxy"));
        }
        for host in &mut self.hosts {
            *host = crate::network::Target::parse(host, false)?.authority();
        }
        for local in &mut self.local {
            *local = crate::network::Target::parse(local, true)?.authority();
        }
        self.hosts.sort();
        self.hosts.dedup();
        self.local.sort();
        self.local.dedup();
        self.publish.sort_unstable();
        self.publish.dedup();
        if self.publish.contains(&0) {
            return Err(io::Error::other(
                "Published ports must be between 1 and 65535",
            ));
        }
        for local in &self.local {
            if self
                .publish
                .contains(&crate::network::Target::parse(local, true)?.port)
            {
                return Err(io::Error::other(
                    "Local forwarding and publishing cannot use the same guest port",
                ));
            }
        }
        if (!self.read.is_empty() || !self.write.is_empty()) && self.mode == Mode::Unrestricted {
            return Err(io::Error::other(
                "Path grants require --workspace or --isolated",
            ));
        }
        if self.read_only && !self.write.is_empty() {
            return Err(io::Error::other(
                "--read-only cannot be combined with writable path grants",
            ));
        }
        if !self.isolated
            && (self.cpus.is_some()
                || self.memory_mib.is_some()
                || self.pids.is_some()
                || self.cgroup_root.is_some())
        {
            return Err(io::Error::other("Resource controls require --isolated"));
        }
        for limit in [self.cpus, self.memory_mib, self.pids]
            .into_iter()
            .flatten()
        {
            if limit == 0 {
                return Err(io::Error::other(
                    "Resource limits must be positive integers",
                ));
            }
        }
        if self.isolated {
            self.cpus.get_or_insert(2);
            self.memory_mib.get_or_insert(2048);
            self.pids.get_or_insert(256);
        }
        for name in &self.pass_env {
            if name.is_empty()
                || !name
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
                || name.starts_with(|ch: char| ch.is_ascii_digit())
            {
                return Err(io::Error::other(
                    "Environment names must use letters, digits, and underscores",
                ));
            }
        }
        if self.mode != Mode::Unrestricted
            && (workspace.parent().is_none()
                || ["/sys", "/proc", "/dev"]
                    .iter()
                    .any(|root| workspace.starts_with(root)))
        {
            return Err(io::Error::other(
                "Choose a project workspace outside filesystem roots and kernel control directories",
            ));
        }
        for path in self.read.iter_mut().chain(&mut self.write) {
            let text = path.to_string_lossy();
            let expanded = if text == "$WORKSPACE" || text.starts_with("$WORKSPACE/") {
                workspace.join(text.strip_prefix("$WORKSPACE/").unwrap_or(""))
            } else if text == "$HOME" || text.starts_with("$HOME/") {
                home()?.join(text.strip_prefix("$HOME/").unwrap_or(""))
            } else if text.starts_with('$') {
                return Err(io::Error::other(
                    "Only $HOME and $WORKSPACE path variables are supported",
                ));
            } else if path.is_relative() {
                workspace.join(&*path)
            } else {
                path.clone()
            };
            *path = expanded.canonicalize().map_err(|error| {
                io::Error::other(format!(
                    "Cannot grant {}: {error}; grant paths must already exist",
                    expanded.display()
                ))
            })?;
        }
        self.read.sort();
        self.read.dedup();
        self.write.sort();
        self.write.dedup();
        Ok(())
    }

    pub fn environment(&self, command: &mut Command) {
        if self.clean_env || self.isolated {
            command.env_clear();
            for (name, value) in std::env::vars_os() {
                if forwarded(&name.to_string_lossy())
                    || self.pass_env.iter().any(|key| name == key.as_str())
                {
                    command.env(name, value);
                }
            }
        }
        if self.solmu {
            let workspace = command
                .get_current_dir()
                .expect("resolved workspace")
                .to_owned();
            command.env("SOLMU_WORKSPACE", workspace);
        }
    }
}

fn home() -> io::Result<PathBuf> {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .map(PathBuf::from)
        .ok_or_else(|| io::Error::other("Cannot resolve $HOME"))
}

pub fn forwarded(name: &str) -> bool {
    matches!(
        name,
        "PATH"
            | "TERM"
            | "LANG"
            | "LC_ALL"
            | "COLORTERM"
            | "SYSTEMROOT"
            | "SystemRoot"
            | "WINDIR"
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
}

pub fn executable(command: &Command) -> io::Result<PathBuf> {
    let directory = command.get_current_dir().expect("resolved workspace");
    let requested = Path::new(command.get_program());
    if requested.is_absolute() {
        return requested.canonicalize();
    }
    if requested.components().count() > 1 {
        return directory.join(requested).canonicalize();
    }
    let environment_path = std::env::var_os("PATH").unwrap_or_default();
    let search = std::env::split_paths(&environment_path);
    for path in search {
        let path = if path.is_relative() {
            directory.join(path)
        } else {
            path
        };
        let candidate = path.join(requested);
        if candidate.is_file() {
            return candidate.canonicalize();
        }
        #[cfg(windows)]
        if requested.extension().is_none() {
            let candidate = candidate.with_extension("exe");
            if candidate.is_file() {
                return candidate.canonicalize();
            }
        }
    }
    Err(io::Error::other("Program was not found on PATH"))
}

pub fn runtime_paths() -> Vec<PathBuf> {
    #[cfg(target_os = "linux")]
    let paths = [
        "/usr",
        "/bin",
        "/sbin",
        "/lib",
        "/lib64",
        "/etc/ld.so.cache",
        "/etc/resolv.conf",
        "/etc/hosts",
        "/etc/nsswitch.conf",
        "/etc/gai.conf",
        "/etc/ssl",
        "/etc/pki",
        "/etc/fonts",
        "/etc/localtime",
        "/proc/self",
        "/proc/meminfo",
        "/proc/cpuinfo",
    ];
    #[cfg(target_os = "macos")]
    let paths = [
        "/System",
        "/usr",
        "/bin",
        "/sbin",
        "/private/etc/resolv.conf",
        "/private/etc/hosts",
        "/private/etc/localtime",
        "/private/etc/ssl",
        "/Library/Apple/System/Library",
        "/Library/Preferences/com.apple.security.plist",
    ];
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    let paths: [&str; 0] = [];
    paths
        .into_iter()
        .filter_map(|path| Path::new(path).canonicalize().ok())
        .collect()
}

pub const ISOLATED_RUNTIME: &[&str] = &[
    "/usr",
    "/bin",
    "/sbin",
    "/lib",
    "/lib64",
    "/etc/ld.so.cache",
    "/etc/resolv.conf",
    "/etc/hosts",
    "/etc/nsswitch.conf",
    "/etc/gai.conf",
    "/etc/ssl",
    "/etc/pki",
    "/etc/fonts",
    "/etc/localtime",
];

pub fn runtime_list() -> Vec<PathBuf> {
    if cfg!(target_os = "macos") {
        // The macOS loader opens the root directory before locating its cache.
        // This is a literal directory grant, never a recursive filesystem grant.
        vec![PathBuf::from("/")]
    } else {
        Vec::new()
    }
}

pub fn device_paths() -> Vec<PathBuf> {
    [
        "/dev/null",
        "/dev/zero",
        "/dev/random",
        "/dev/urandom",
        "/dev/tty",
    ]
    .into_iter()
    .filter_map(|path| Path::new(path).canonicalize().ok())
    .collect()
}
