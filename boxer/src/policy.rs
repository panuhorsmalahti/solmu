use serde::{Deserialize, Serialize};
use std::{
    io::{self, Read},
    path::{Path, PathBuf},
    process::Command,
};
use zeroize::Zeroizing;

fn serialize_upstream_proxy<S>(
    value: &Option<Zeroizing<String>>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    let sanitized = value
        .as_ref()
        .map(|value| {
            let mut url = url::Url::parse(value).map_err(serde::ser::Error::custom)?;
            url.set_username("")
                .map_err(|()| serde::ser::Error::custom("Invalid upstream proxy URL"))?;
            url.set_password(None)
                .map_err(|()| serde::ser::Error::custom("Invalid upstream proxy URL"))?;
            Ok::<_, S::Error>(url.to_string())
        })
        .transpose()?;
    sanitized.serialize(serializer)
}

fn deserialize_upstream_proxy<'de, D>(
    deserializer: D,
) -> Result<Option<Zeroizing<String>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Option::<String>::deserialize(deserializer).map(|value| value.map(Zeroizing::new))
}

fn serialize_redacted_environment<S>(
    values: &std::collections::BTreeMap<String, String>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    values
        .keys()
        .map(|key| (key, "[redacted]"))
        .collect::<std::collections::BTreeMap<_, _>>()
        .serialize(serializer)
}

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

#[derive(Clone, Copy, PartialEq)]
pub enum AgentProfile {
    Codex,
    ClaudeCode,
    OpenCode,
    Pi,
}

#[derive(Clone, Copy, Deserialize, Serialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum RuntimeGroup {
    Node,
    Python,
    Rust,
    Go,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum CredentialProvider {
    Openai,
    Anthropic,
    Gemini,
    Github,
    Gitlab,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, Hash)]
#[serde(deny_unknown_fields)]
pub struct EndpointRule {
    pub provider: String,
    pub method: String,
    pub path: String,
}

impl EndpointRule {
    pub fn parse(value: &str) -> io::Result<Self> {
        let mut fields = value.splitn(3, ':');
        let provider = fields.next().unwrap_or_default().to_owned();
        validate_credential_name(&provider)?;
        let method = fields.next().unwrap_or_default().to_owned();
        let path = fields.next().unwrap_or_default().to_owned();
        let rule = Self {
            provider,
            method,
            path,
        };
        rule.validate()?;
        Ok(rule)
    }

    pub fn validate(&self) -> io::Result<()> {
        validate_credential_name(&self.provider)?;
        if self.method != "*"
            && (!http_token(&self.method)
                || self.method.bytes().any(|byte| byte.is_ascii_lowercase()))
        {
            return Err(io::Error::other(
                "Endpoint methods must be uppercase HTTP tokens or *",
            ));
        }
        if !self.path.starts_with('/')
            || self.path.starts_with("//")
            || self
                .path
                .chars()
                .any(|character| matches!(character, '?' | '#' | '%' | '\\'))
            || self
                .path
                .bytes()
                .any(|byte| byte.is_ascii_control() || byte == b' ')
        {
            return Err(io::Error::other(
                "Endpoint paths must be absolute paths without queries or encoded characters",
            ));
        }
        let segments: Vec<_> = self.path.split('/').skip(1).collect();
        if segments.len() > 128
            || segments.iter().enumerate().any(|(index, segment)| {
                (segment.is_empty() && index + 1 != segments.len())
                    || matches!(*segment, "." | "..")
                    || (segment.contains('*') && !matches!(*segment, "*" | "**"))
            })
        {
            return Err(io::Error::other("Invalid endpoint path pattern"));
        }
        Ok(())
    }

    #[cfg(any(target_os = "linux", test))]
    pub fn matches(&self, provider: &str, method: &str, path: &str) -> bool {
        if self.provider != provider || (self.method != "*" && self.method != method) {
            return false;
        }
        let pattern: Vec<_> = self.path.split('/').skip(1).collect();
        let path: Vec<_> = path.split('/').skip(1).collect();
        let mut matched = vec![false; path.len() + 1];
        matched[0] = true;
        for segment in pattern {
            let mut next = vec![false; path.len() + 1];
            if segment == "**" {
                let mut reachable = false;
                for (index, is_matched) in matched.iter().enumerate() {
                    reachable |= *is_matched;
                    next[index] = reachable;
                }
            } else {
                for (index, value) in path.iter().enumerate() {
                    if matched[index]
                        && ((segment == "*" && !value.is_empty()) || segment == *value)
                    {
                        next[index + 1] = true;
                    }
                }
            }
            matched = next;
        }
        matched[path.len()]
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CustomCredential {
    pub upstream: String,
    pub credential_key: String,
    #[serde(default)]
    pub env_var: Option<String>,
    #[serde(default)]
    pub inject_mode: CredentialInjectionMode,
    #[serde(default = "default_credential_header")]
    pub inject_header: String,
    #[serde(default = "default_credential_format")]
    pub credential_format: String,
    #[serde(default)]
    pub path_pattern: Option<String>,
    #[serde(default)]
    pub path_replacement: Option<String>,
    #[serde(default)]
    pub query_param_name: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CredentialInjectionMode {
    #[default]
    Header,
    UrlPath,
    QueryParam,
    BasicAuth,
}

fn default_credential_header() -> String {
    "Authorization".to_owned()
}

fn default_credential_format() -> String {
    "Bearer {}".to_owned()
}

fn validate_credential_name(name: &str) -> io::Result<()> {
    if name.is_empty()
        || name.len() > 64
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
        || !name.as_bytes()[0].is_ascii_lowercase()
    {
        return Err(io::Error::other(
            "Credential route names must be lowercase identifiers",
        ));
    }
    Ok(())
}

fn validate_custom_credential_name(name: &str) -> io::Result<()> {
    validate_credential_name(name)
}

impl CustomCredential {
    pub fn token_env(&self, name: &str) -> String {
        self.env_var
            .clone()
            .unwrap_or_else(|| format!("{}_API_KEY", name.to_ascii_uppercase()))
    }

    pub fn base_env(name: &str) -> String {
        format!("{}_BASE_URL", name.to_ascii_uppercase())
    }

    pub fn validate(&self, name: &str) -> io::Result<String> {
        validate_custom_credential_name(name)?;
        if !crate::credential::valid_source_key(&self.credential_key) {
            return Err(io::Error::other(format!(
                "Invalid credential key for custom route {name}"
            )));
        }
        let token_env = self.token_env(name);
        if !crate::credential::valid_name(&token_env) {
            return Err(io::Error::other(format!(
                "Invalid phantom-token environment name for custom route {name}"
            )));
        }
        let base_env = Self::base_env(name);
        if !crate::credential::valid_name(&base_env) {
            return Err(io::Error::other(format!(
                "Invalid base URL environment name for custom route {name}"
            )));
        }
        if token_env.eq_ignore_ascii_case(&base_env) {
            return Err(io::Error::other(format!(
                "Phantom-token and base URL variables collide for custom route {name}"
            )));
        }
        if !http_token(&self.inject_header)
            || [
                "host",
                "connection",
                "proxy-connection",
                "proxy-authorization",
                "content-length",
                "transfer-encoding",
                "expect",
            ]
            .iter()
            .any(|blocked| self.inject_header.eq_ignore_ascii_case(blocked))
            || self
                .inject_header
                .to_ascii_lowercase()
                .starts_with("proxy-")
        {
            return Err(io::Error::other(format!(
                "Invalid credential injection header for custom route {name}"
            )));
        }
        if self.inject_mode == CredentialInjectionMode::Header
            && (self.credential_format.matches("{}").count() != 1
                || self
                    .credential_format
                    .chars()
                    .any(|character| character.is_ascii_control()))
        {
            return Err(io::Error::other(format!(
                "Credential format for custom route {name} must contain one {{}} placeholder"
            )));
        }
        match self.inject_mode {
            CredentialInjectionMode::Header | CredentialInjectionMode::BasicAuth => {}
            CredentialInjectionMode::UrlPath => {
                let pattern = self.path_pattern.as_deref().ok_or_else(|| {
                    io::Error::other(format!("Custom route {name} requires path_pattern"))
                })?;
                validate_path_template(pattern, name, "path_pattern")?;
                if let Some(replacement) = &self.path_replacement {
                    validate_path_template(replacement, name, "path_replacement")?;
                }
            }
            CredentialInjectionMode::QueryParam => {
                let parameter = self.query_param_name.as_deref().ok_or_else(|| {
                    io::Error::other(format!("Custom route {name} requires query_param_name"))
                })?;
                if parameter.is_empty()
                    || parameter.len() > 128
                    || parameter.bytes().any(|byte| {
                        byte.is_ascii_control() || matches!(byte, b'&' | b'=' | b'#' | b'%')
                    })
                {
                    return Err(io::Error::other(format!(
                        "Invalid query_param_name for custom route {name}"
                    )));
                }
            }
        }
        let upstream = url::Url::parse(&self.upstream).map_err(|_| {
            io::Error::other(format!("Invalid upstream URL for custom route {name}"))
        })?;
        if upstream.scheme() != "https"
            || upstream.host_str().is_none()
            || !upstream.username().is_empty()
            || upstream.password().is_some()
            || upstream.query().is_some()
            || upstream.fragment().is_some()
            || upstream.path().contains('%')
        {
            return Err(io::Error::other(format!(
                "Custom credential upstream for {name} must be an HTTPS URL without credentials, query, fragment, or encoded path"
            )));
        }
        let host = upstream.host_str().unwrap();
        let authority = match upstream.port() {
            Some(port) => format!("{host}:{port}"),
            None => format!("{host}:443"),
        };
        Ok(crate::network::HostPattern::parse(&authority)?.authority())
    }
}

fn validate_path_template(value: &str, name: &str, field: &str) -> io::Result<()> {
    if value.matches("{}").count() != 1
        || !value.starts_with('/')
        || value.starts_with("//")
        || value.chars().any(|character| {
            matches!(character, '?' | '#' | '%' | '\\') || character.is_ascii_control()
        })
        || value
            .split('/')
            .any(|segment| matches!(segment, "." | ".."))
    {
        return Err(io::Error::other(format!(
            "Custom route {name} has an invalid {field} template"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod custom_credential_tests {
    use super::*;

    fn sample() -> CustomCredential {
        CustomCredential {
            upstream: "https://api.example.com/v1".to_owned(),
            credential_key: "EXAMPLE_API_KEY".to_owned(),
            env_var: None,
            inject_mode: CredentialInjectionMode::Header,
            inject_header: "Authorization".to_owned(),
            credential_format: "Bearer {}".to_owned(),
            path_pattern: None,
            path_replacement: None,
            query_param_name: None,
        }
    }

    #[test]
    fn custom_routes_validate_and_generate_safe_host_and_environment_names() {
        let custom = sample();
        assert_eq!(
            custom.validate("example_api").unwrap(),
            "api.example.com:443"
        );
        assert_eq!(custom.token_env("example_api"), "EXAMPLE_API_API_KEY");
        assert_eq!(
            CustomCredential::base_env("example_api"),
            "EXAMPLE_API_BASE_URL"
        );
    }

    #[test]
    fn custom_routes_reject_unsafe_upstreams_headers_and_token_formats() {
        let mut custom = sample();
        custom.upstream = "http://api.example.com".to_owned();
        assert!(custom.validate("example_api").is_err());
        custom = sample();
        custom.inject_header = "Host".to_owned();
        assert!(custom.validate("example_api").is_err());
        custom = sample();
        custom.credential_format = "Bearer {} {}".to_owned();
        assert!(custom.validate("example_api").is_err());
        custom = sample();
        custom.upstream = "https://user:secret@api.example.com".to_owned();
        assert!(custom.validate("example_api").is_err());
    }

    #[test]
    fn custom_routes_validate_path_query_and_basic_injection_modes() {
        let mut custom = sample();
        custom.inject_mode = CredentialInjectionMode::UrlPath;
        assert!(custom.validate("example_api").is_err());
        custom.path_pattern = Some("/bot{}/".to_owned());
        assert!(custom.validate("example_api").is_ok());
        custom.path_replacement = Some("/v2/bot{}/".to_owned());
        assert!(custom.validate("example_api").is_ok());
        custom.path_replacement = Some("/v2/../bot{}/".to_owned());
        assert!(custom.validate("example_api").is_err());

        custom = sample();
        custom.inject_mode = CredentialInjectionMode::QueryParam;
        assert!(custom.validate("example_api").is_err());
        custom.query_param_name = Some("key".to_owned());
        assert!(custom.validate("example_api").is_ok());

        custom = sample();
        custom.inject_mode = CredentialInjectionMode::BasicAuth;
        assert!(custom.validate("example_api").is_ok());
    }
}

fn http_token(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&byte))
}

#[cfg(test)]
mod endpoint_rule_tests {
    use super::*;

    #[test]
    fn path_patterns_match_one_or_many_segments() {
        let rule = EndpointRule::parse("openai:POST:/v1/models/*/responses/**").unwrap();
        assert!(rule.matches("openai", "POST", "/v1/models/gpt-6/responses"));
        assert!(rule.matches("openai", "POST", "/v1/models/gpt-6/responses/stream"));
        assert!(!rule.matches("openai", "GET", "/v1/models/gpt-6/responses"));
        assert!(!rule.matches("anthropic", "POST", "/v1/models/gpt-6/responses"));
    }

    #[test]
    fn endpoint_patterns_reject_ambiguous_paths_and_methods() {
        for invalid in [
            "openai:get:/v1/models",
            "openai:POST:v1/models",
            "openai:POST:/v1/*/../admin",
            "openai:POST:/v1/foo*",
            "openai:GET :/v1/models",
            "openai:POST:/v1/models?verbose=true",
            "openai:POST:/v1/%6dodels",
        ] {
            assert!(EndpointRule::parse(invalid).is_err(), "accepted {invalid}");
        }
        assert!(EndpointRule::parse("openai:*:/v1/**").is_ok());
    }
}

impl CredentialProvider {
    pub fn parse(name: &str) -> io::Result<Self> {
        match name {
            "openai" => Ok(Self::Openai),
            "anthropic" => Ok(Self::Anthropic),
            "gemini" => Ok(Self::Gemini),
            "github" => Ok(Self::Github),
            "gitlab" => Ok(Self::Gitlab),
            _ => Err(io::Error::other(
                "Unknown credential provider; available providers: openai, anthropic, gemini, github, gitlab",
            )),
        }
    }

    pub fn key_env(self) -> &'static str {
        match self {
            Self::Openai => "OPENAI_API_KEY",
            Self::Anthropic => "ANTHROPIC_API_KEY",
            Self::Gemini => "GEMINI_API_KEY",
            Self::Github => "GITHUB_TOKEN",
            Self::Gitlab => "GITLAB_TOKEN",
        }
    }

    pub fn base_env(self) -> &'static str {
        match self {
            Self::Openai => "OPENAI_BASE_URL",
            Self::Anthropic => "ANTHROPIC_BASE_URL",
            Self::Gemini => "GEMINI_BASE_URL",
            Self::Github => "GITHUB_API_URL",
            Self::Gitlab => "GITLAB_API_URL",
        }
    }

    pub fn host(self) -> &'static str {
        match self {
            Self::Openai => "api.openai.com:443",
            Self::Anthropic => "api.anthropic.com:443",
            Self::Gemini => "generativelanguage.googleapis.com:443",
            Self::Github => "api.github.com:443",
            Self::Gitlab => "gitlab.com:443",
        }
    }

    #[cfg(all(target_os = "linux", test))]
    pub fn name(self) -> &'static str {
        match self {
            Self::Openai => "openai",
            Self::Anthropic => "anthropic",
            Self::Gemini => "gemini",
            Self::Github => "github",
            Self::Gitlab => "gitlab",
        }
    }

    #[cfg(all(target_os = "linux", test))]
    pub fn route(self) -> &'static str {
        self.name()
    }
}

impl RuntimeGroup {
    pub fn parse(name: &str) -> io::Result<Self> {
        match name {
            "node" => Ok(Self::Node),
            "python" => Ok(Self::Python),
            "rust" => Ok(Self::Rust),
            "go" => Ok(Self::Go),
            _ => Err(io::Error::other(
                "Unknown runtime group; available groups: node, python, rust, go",
            )),
        }
    }

    fn paths(self, home: &Path) -> Vec<PathBuf> {
        let env_path = |name: &str, fallback: PathBuf| {
            std::env::var_os(name)
                .map(PathBuf::from)
                .unwrap_or(fallback)
        };
        match self {
            Self::Node => [
                Some(env_path("NVM_DIR", home.join(".nvm"))),
                Some(env_path("FNM_DIR", home.join(".fnm"))),
                Some(env_path("VOLTA_HOME", home.join(".volta"))),
                Some(env_path("PNPM_HOME", home.join(".local/share/pnpm"))),
                Some(home.join(".bun")),
                Some(home.join(".npm")),
                Some(home.join(".local/share/node")),
            ]
            .into_iter()
            .flatten()
            .collect(),
            Self::Python => [
                Some(env_path("PYENV_ROOT", home.join(".pyenv"))),
                Some(env_path("CONDA_ENVS_PATH", home.join(".conda/envs"))),
                Some(home.join(".conda/pkgs")),
                Some(home.join(".local/share/uv")),
                Some(home.join(".cache/uv")),
                Some(home.join(".cache/pip")),
            ]
            .into_iter()
            .flatten()
            .collect(),
            Self::Rust => {
                let cargo = env_path("CARGO_HOME", home.join(".cargo"));
                let mut paths = vec![
                    cargo.join("bin"),
                    cargo.join("registry"),
                    cargo.join("git"),
                    env_path("RUSTUP_HOME", home.join(".rustup")),
                ];
                paths.retain(|path| path.exists());
                paths
            }
            Self::Go => {
                let mut paths = std::env::var_os("GOPATH")
                    .map(|value| std::env::split_paths(&value).collect::<Vec<_>>())
                    .filter(|paths| !paths.is_empty())
                    .unwrap_or_else(|| vec![home.join("go")]);
                if let Some(root) = std::env::var_os("GOROOT") {
                    paths.push(PathBuf::from(root));
                } else {
                    paths.push(PathBuf::from("/usr/local/go"));
                }
                paths
            }
        }
    }
}

impl AgentProfile {
    pub fn name(self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::ClaudeCode => "claude-code",
            Self::OpenCode => "opencode",
            Self::Pi => "pi",
        }
    }
}

#[derive(Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Policy {
    pub mode: Mode,
    pub network: Network,
    pub network_profile: Option<String>,
    #[serde(
        serialize_with = "serialize_upstream_proxy",
        deserialize_with = "deserialize_upstream_proxy"
    )]
    pub upstream_proxy: Option<Zeroizing<String>>,
    pub upstream_bypass: Vec<String>,
    pub hosts: Vec<String>,
    pub deny_hosts: Vec<String>,
    pub local: Vec<String>,
    pub publish: Vec<u16>,
    pub proxy_port: Option<u16>,
    pub read_only: bool,
    pub read: Vec<PathBuf>,
    pub write: Vec<PathBuf>,
    pub write_only: Vec<PathBuf>,
    pub clean_env: bool,
    pub pass_env: Vec<String>,
    pub environment: Option<EnvironmentPolicy>,
    pub env_credentials: Vec<String>,
    pub env_credential_map: std::collections::BTreeMap<String, String>,
    pub runtime_groups: Vec<RuntimeGroup>,
    pub credentials: Vec<String>,
    #[serde(default)]
    pub custom_credentials: std::collections::BTreeMap<String, CustomCredential>,
    pub endpoint_rules: Vec<EndpointRule>,
    pub cpus: Option<u32>,
    pub memory_mib: Option<u32>,
    pub pids: Option<u32>,
    pub cgroup_root: Option<PathBuf>,
    #[serde(skip)]
    pub isolated: bool,
    #[serde(skip)]
    pub solmu: bool,
    #[serde(skip)]
    pub agent: Option<AgentProfile>,
    #[serde(skip)]
    pub profile_home: Option<PathBuf>,
}

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct EnvironmentPolicy {
    /// `None` inherits all otherwise permitted variables; `Some([])` inherits none.
    pub allow_vars: Option<Vec<String>>,
    pub deny_vars: Vec<String>,
    pub case_insensitive_vars: bool,
    #[serde(serialize_with = "serialize_redacted_environment")]
    pub set_vars: std::collections::BTreeMap<String, String>,
}

impl EnvironmentPolicy {
    fn validate(&self) -> io::Result<()> {
        for pattern in self.allow_vars.iter().flatten().chain(&self.deny_vars) {
            if pattern.is_empty() || pattern.contains('\0') {
                return Err(io::Error::other(
                    "Environment variable patterns cannot be empty or contain NUL",
                ));
            }
        }
        for (name, value) in &self.set_vars {
            if !crate::credential::valid_name(name)
                || name.eq_ignore_ascii_case("PATH")
                || name.to_ascii_uppercase().starts_with("BOXER_")
            {
                return Err(io::Error::other(format!(
                    "Invalid or reserved environment.set_vars key: {name}"
                )));
            }
            if value.contains('\0') {
                return Err(io::Error::other(format!(
                    "Environment value for {name} cannot contain NUL"
                )));
            }
        }
        Ok(())
    }

    fn matches_any(&self, patterns: &[String], name: &str) -> bool {
        patterns.iter().any(|pattern| {
            let (pattern, name) = if self.case_insensitive_vars {
                (pattern.to_ascii_lowercase(), name.to_ascii_lowercase())
            } else {
                (pattern.clone(), name.to_owned())
            };
            glob_env_name(&pattern, &name)
        })
    }
}

fn glob_env_name(pattern: &str, name: &str) -> bool {
    // `*` matches any run of characters. Dynamic programming avoids recursive
    // backtracking for user supplied patterns.
    let mut matched = vec![false; name.len() + 1];
    matched[0] = true;
    for byte in pattern.bytes() {
        let mut next = vec![false; name.len() + 1];
        if byte == b'*' {
            let mut reachable = false;
            for index in 0..=name.len() {
                reachable |= matched[index];
                next[index] = reachable;
            }
        } else {
            for (index, candidate) in name.bytes().enumerate() {
                if matched[index] && candidate == byte {
                    next[index + 1] = true;
                }
            }
        }
        matched = next;
    }
    matched[name.len()]
}

fn dangerous_inherited_env(name: &str) -> bool {
    let name = name.to_ascii_uppercase();
    matches!(
        name.as_str(),
        "LD_PRELOAD"
            | "LD_LIBRARY_PATH"
            | "DYLD_INSERT_LIBRARIES"
            | "DYLD_LIBRARY_PATH"
            | "PYTHONPATH"
            | "PYTHONHOME"
            | "NODE_OPTIONS"
    )
}

impl Policy {
    pub fn from_file(path: &Path) -> io::Result<Self> {
        Self::from_file_with_path_resolution(path, true)
    }

    pub fn from_file_raw(path: &Path) -> io::Result<Self> {
        Self::from_file_with_path_resolution(path, false)
    }

    pub fn validate_parent(child_path: &Path, parent: &str) -> io::Result<()> {
        let path = inherited_policy_path(child_path, parent)?;
        Self::from_file_raw(&path).map(|_| ())
    }

    fn from_file_with_path_resolution(path: &Path, resolve_paths: bool) -> io::Result<Self> {
        let mut ancestors = std::collections::HashSet::new();
        let object = load_policy_chain(path, &mut ancestors, 0, resolve_paths)?;
        if !object.contains_key("mode") {
            return Err(io::Error::other(
                "Policy requires an explicit mode: unrestricted, workspace, or isolated",
            ));
        }
        let policy: Self =
            serde_json::from_value(serde_json::Value::Object(object)).map_err(io::Error::other)?;
        Ok(policy)
    }

    pub fn resolve(&mut self, workspace: &Path) -> io::Result<()> {
        self.isolated = self.mode == Mode::Isolated;
        if self.upstream_proxy.is_none() {
            self.upstream_proxy = std::env::var("BOXER_UPSTREAM_PROXY")
                .ok()
                .map(Zeroizing::new);
        }
        if self.upstream_bypass.is_empty()
            && let Ok(value) = std::env::var("BOXER_UPSTREAM_BYPASS")
        {
            self.upstream_bypass = value
                .split(',')
                .map(str::trim)
                .filter(|pattern| !pattern.is_empty())
                .map(str::to_owned)
                .collect();
        }
        if let Some(proxy) = &self.upstream_proxy {
            crate::network::UpstreamProxy::parse(proxy)?;
            if self.network != Network::Proxy || !self.isolated {
                return Err(io::Error::other(
                    "An upstream proxy requires Linux --isolated --network proxy",
                ));
            }
        } else if !self.upstream_bypass.is_empty() {
            return Err(io::Error::other(
                "Upstream bypass rules require an upstream proxy",
            ));
        }
        for pattern in &mut self.upstream_bypass {
            *pattern = crate::network::normalize_bypass_pattern(pattern)?;
        }
        self.resolve_rest(workspace)
    }

    fn resolve_rest(&mut self, workspace: &Path) -> io::Result<()> {
        self.upstream_bypass.sort();
        self.upstream_bypass.dedup();
        if let Some(profile) = &self.network_profile {
            if self.network != Network::Proxy {
                return Err(io::Error::other("Network profiles require --network proxy"));
            }
            self.hosts.extend(crate::network::profile_hosts(profile)?);
        }
        for (name, credential) in &self.custom_credentials {
            credential.validate(name)?;
        }
        if !self.credentials.is_empty() {
            if !cfg!(target_os = "linux") {
                return Err(io::Error::other(
                    "Credential proxying is currently supported on Linux",
                ));
            }
            if !self.isolated || self.network != Network::Proxy {
                return Err(io::Error::other(
                    "Credential proxying requires Linux --isolated --network proxy",
                ));
            }
            let mut providers = std::collections::HashSet::new();
            let mut route_environment = std::collections::HashSet::new();
            for name in &self.credentials {
                validate_credential_name(name)?;
                if !providers.insert(name) {
                    return Err(io::Error::other(
                        "A credential provider was specified more than once",
                    ));
                }
                let (token_env, base_env, host) = if let Some(custom) =
                    self.custom_credentials.get(name)
                {
                    (
                        custom.token_env(name),
                        CustomCredential::base_env(name),
                        custom.validate(name)?,
                    )
                } else if let Ok(provider) = CredentialProvider::parse(name) {
                    (
                        provider.key_env().to_owned(),
                        provider.base_env().to_owned(),
                        provider.host().to_owned(),
                    )
                } else {
                    return Err(io::Error::other(format!(
                        "Unknown credential route {name}; add a custom_credentials definition or choose a built-in route"
                    )));
                };
                for variable in [&token_env, &base_env] {
                    if self
                        .env_credentials
                        .iter()
                        .any(|env| env.eq_ignore_ascii_case(variable))
                        || self
                            .env_credential_map
                            .values()
                            .any(|env| env.eq_ignore_ascii_case(variable))
                    {
                        return Err(io::Error::other(format!(
                            "Credential route {name} conflicts with --env-credential {variable}"
                        )));
                    }
                    if !route_environment.insert(variable.to_ascii_uppercase()) {
                        return Err(io::Error::other(format!(
                            "Credential routes have conflicting environment variable {variable}"
                        )));
                    }
                }
                self.hosts.push(host);
            }
            if self.solmu && self.credentials.len() != 1 {
                return Err(io::Error::other(
                    "The Solmu profile accepts one brokered provider at a time",
                ));
            }
        }
        if !self.endpoint_rules.is_empty() {
            if self.credentials.is_empty() {
                return Err(io::Error::other(
                    "Endpoint allowlists require at least one --credential provider",
                ));
            }
            let mut rules = std::collections::HashSet::new();
            for rule in &self.endpoint_rules {
                rule.validate()?;
                if !self.credentials.contains(&rule.provider) {
                    return Err(io::Error::other(format!(
                        "Endpoint rule provider {} is not configured with --credential",
                        rule.provider
                    )));
                }
                if !rules.insert(rule) {
                    return Err(io::Error::other(
                        "An endpoint rule was specified more than once",
                    ));
                }
            }
        }
        if self.network == Network::Proxy && !self.isolated {
            return Err(io::Error::other(
                "Proxy networking requires Linux --isolated mode",
            ));
        }
        if !self.runtime_groups.is_empty() && self.mode == Mode::Unrestricted {
            return Err(io::Error::other(
                "Runtime groups require --workspace or --isolated",
            ));
        }
        let mut groups = std::collections::HashSet::new();
        for group in &self.runtime_groups {
            if !groups.insert(*group) {
                return Err(io::Error::other(
                    "A runtime group was specified more than once",
                ));
            }
        }
        if !self.runtime_groups.is_empty() {
            let home = home()?;
            for group in &self.runtime_groups {
                self.read
                    .extend(group.paths(&home).into_iter().filter(|path| path.exists()));
            }
        }
        if self.network != Network::Proxy
            && (!self.hosts.is_empty()
                || !self.deny_hosts.is_empty()
                || !self.local.is_empty()
                || !self.publish.is_empty())
        {
            return Err(io::Error::other("Network routes require --network proxy"));
        }
        for host in &mut self.hosts {
            *host = crate::network::HostPattern::parse(host)?.authority();
        }
        for host in &mut self.deny_hosts {
            *host = crate::network::normalize_domain_pattern(host)?;
        }
        self.deny_hosts.sort();
        self.deny_hosts.dedup();
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
        if let Some(port) = self.proxy_port {
            if port == 0 {
                return Err(io::Error::other(
                    "--proxy-port requires a port from 1 to 65535",
                ));
            }
            if self.network != Network::Proxy || !self.isolated {
                return Err(io::Error::other(
                    "--proxy-port requires Linux --isolated --network proxy",
                ));
            }
            if self.publish.contains(&port)
                || self.local.iter().any(|route| {
                    crate::network::Target::parse(route, true)
                        .is_ok_and(|target| target.port == port)
                })
            {
                return Err(io::Error::other(
                    "The network proxy port conflicts with another routed port",
                ));
            }
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
        if (!self.read.is_empty() || !self.write.is_empty() || !self.write_only.is_empty())
            && self.mode == Mode::Unrestricted
        {
            return Err(io::Error::other(
                "Path grants require --workspace or --isolated",
            ));
        }
        if self.read_only && (!self.write.is_empty() || !self.write_only.is_empty()) {
            return Err(io::Error::other(
                "--read-only cannot be combined with writable path grants",
            ));
        }
        if !self.write_only.is_empty() && (self.isolated || cfg!(windows)) {
            return Err(io::Error::other(
                "Write-only path grants require workspace mode on Linux or macOS",
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
        if let Some(environment) = &self.environment {
            environment.validate()?;
        }
        let mut credentials = std::collections::HashSet::new();
        for name in &self.env_credentials {
            if !crate::credential::valid_name(name) {
                return Err(io::Error::other(format!(
                    "Invalid --env-credential environment variable name: {name}"
                )));
            }
            if !credentials.insert(name.to_ascii_uppercase()) {
                return Err(io::Error::other(format!(
                    "Credential {name} was specified more than once"
                )));
            }
        }
        for (source, target) in &self.env_credential_map {
            if !crate::credential::valid_source_key(source) {
                return Err(io::Error::other(format!(
                    "Invalid credential source in --env-credential-map: {source}"
                )));
            }
            if !crate::credential::valid_name(target) {
                return Err(io::Error::other(format!(
                    "Invalid target environment variable in --env-credential-map: {target}"
                )));
            }
            if !credentials.insert(target.to_ascii_uppercase()) {
                return Err(io::Error::other(format!(
                    "Credential environment variable {target} was specified more than once"
                )));
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
        for path in self
            .read
            .iter_mut()
            .chain(&mut self.write)
            .chain(&mut self.write_only)
        {
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
        self.write_only.sort();
        self.write_only.dedup();
        let mut readable = runtime_paths();
        readable.push(workspace.to_owned());
        readable.extend(self.read.iter().cloned());
        readable.extend(self.write.iter().cloned());
        readable.extend(device_paths());
        self.validate_write_only_overlaps(&readable)?;
        Ok(())
    }

    pub fn validate_write_only_overlaps(&self, readable: &[PathBuf]) -> io::Result<()> {
        for path in &self.write_only {
            if readable
                .iter()
                .any(|grant| path.starts_with(grant) || grant.starts_with(path))
            {
                return Err(io::Error::other(format!(
                    "Write-only grant {} overlaps another readable or writable path grant",
                    path.display()
                )));
            }
        }
        Ok(())
    }

    pub fn environment(&self, command: &mut Command) -> io::Result<()> {
        let filter_enabled = self.clean_env || self.isolated || self.environment.is_some();
        if filter_enabled {
            command.env_clear();
            for (name, value) in std::env::vars_os() {
                let name_text = name.to_string_lossy();
                let builtin = self.agent.map_or_else(
                    || forwarded(&name.to_string_lossy()),
                    |agent| forwarded_for_agent(&name.to_string_lossy(), agent),
                );
                let explicitly_passed = self.pass_env.iter().any(|key| name == key.as_str());
                let environment = self.environment.as_ref();
                let allow = environment
                    .and_then(|rules| rules.allow_vars.as_ref())
                    .map_or(
                        if self.clean_env || self.isolated {
                            builtin
                        } else {
                            true
                        },
                        |patterns| {
                            environment.unwrap().matches_any(patterns, &name_text)
                                || explicitly_passed
                        },
                    );
                let deny = environment
                    .is_some_and(|rules| rules.matches_any(&rules.deny_vars, &name_text));
                if allow && !deny && !dangerous_inherited_env(&name_text) {
                    command.env(name, value);
                }
            }
        }
        for name in &self.credentials {
            if let Some(custom) = self.custom_credentials.get(name) {
                command.env_remove(custom.token_env(name));
            } else if let Ok(provider) = CredentialProvider::parse(name) {
                command.env_remove(provider.key_env());
            }
        }
        if self.solmu {
            let workspace = command
                .get_current_dir()
                .expect("resolved workspace")
                .to_owned();
            command.env("SOLMU_WORKSPACE", workspace);
        }
        if let (Some(agent), Some(home)) = (self.agent, &self.profile_home) {
            command.env("HOME", home).env("TMPDIR", home.join("tmp"));
            match agent {
                AgentProfile::Codex => {
                    command.env("CODEX_HOME", home.join(".codex"));
                }
                AgentProfile::ClaudeCode => {
                    command.env("CLAUDE_CONFIG_DIR", home.join(".claude"));
                }
                AgentProfile::OpenCode => {
                    command
                        .env("OPENCODE_CONFIG_DIR", home.join("config"))
                        .env("OPENCODE_DATA_DIR", home.join("data"))
                        .env("OPENCODE_CACHE_DIR", home.join("cache"))
                        .env("OPENCODE_LOG_DIR", home.join("log"))
                        .env("OPENCODE_STATE_DIR", home.join("state"));
                }
                AgentProfile::Pi => {
                    command.env("PI_CODING_AGENT_DIR", home.join("agent"));
                }
            }
        }
        if let Some(environment) = &self.environment {
            for (name, value) in &environment.set_vars {
                let expanded = expand_environment_value(value, command)?;
                command.env(name, expanded);
            }
        }
        Ok(())
    }
}

fn expand_environment_value(mut value: &str, command: &Command) -> io::Result<String> {
    let workspace = command
        .get_current_dir()
        .map(PathBuf::from)
        .unwrap_or(std::env::current_dir()?)
        .to_string_lossy()
        .into_owned();
    let mut variables = std::collections::BTreeMap::from([
        ("WORKDIR", workspace),
        (
            "TMPDIR",
            std::env::var_os("TMPDIR")
                .map(PathBuf::from)
                .unwrap_or_else(std::env::temp_dir)
                .to_string_lossy()
                .into_owned(),
        ),
    ]);
    let home = home().ok().map(|home| home.to_string_lossy().into_owned());
    if let Some(home) = &home {
        variables.insert("HOME", home.clone());
    }
    #[cfg(unix)]
    variables.insert("UID", unsafe { libc::geteuid() }.to_string());
    for (name, suffix) in [
        ("XDG_CONFIG_HOME", ".config"),
        ("XDG_DATA_HOME", ".local/share"),
        ("XDG_STATE_HOME", ".local/state"),
        ("XDG_CACHE_HOME", ".cache"),
    ] {
        if let Some(home) = &home {
            variables.insert(
                name,
                std::env::var(name).unwrap_or_else(|_| {
                    Path::new(home).join(suffix).to_string_lossy().into_owned()
                }),
            );
        } else if let Ok(value) = std::env::var(name) {
            variables.insert(name, value);
        }
    }
    if let Some(runtime) = std::env::var_os("XDG_RUNTIME_DIR") {
        variables.insert("XDG_RUNTIME_DIR", runtime.to_string_lossy().into_owned());
    }
    if let Ok(path) = crate::profiles::profile_directory() {
        variables.insert(
            "BOXER_CONFIG",
            path.parent()
                .unwrap_or_else(|| Path::new("."))
                .to_string_lossy()
                .into_owned(),
        );
    }

    let mut expanded = String::with_capacity(value.len());
    if let Some(rest) = value.strip_prefix('~')
        && (rest.is_empty() || rest.starts_with('/') || rest.starts_with('\\'))
    {
        if let Some(home) = &home {
            expanded.push_str(home);
        } else {
            expanded.push('~');
        }
        value = rest;
    }
    let bytes = value.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'$' {
            let character = value[index..].chars().next().expect("valid UTF-8 boundary");
            expanded.push(character);
            index += character.len_utf8();
            continue;
        }
        let start = index + 1;
        let mut end = start;
        while end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
            end += 1;
        }
        if end == start {
            expanded.push('$');
            index += 1;
            continue;
        }
        let name = &value[start..end];
        if let Some(replacement) = variables.get(name) {
            expanded.push_str(replacement);
        } else {
            expanded.push_str(&value[index..end]);
        }
        index = end;
    }
    Ok(expanded)
}

fn load_policy_chain(
    path: &Path,
    ancestors: &mut std::collections::HashSet<PathBuf>,
    depth: usize,
    resolve_paths: bool,
) -> io::Result<serde_json::Map<String, serde_json::Value>> {
    if depth >= 16 {
        return Err(io::Error::other("Policy inheritance exceeds 16 levels"));
    }
    let path = path.canonicalize()?;
    if !ancestors.insert(path.clone()) {
        return Err(io::Error::other(format!(
            "Policy inheritance cycle includes {}",
            path.display()
        )));
    }
    let result = (|| {
        let mut source = Vec::new();
        std::fs::File::open(&path)?
            .take(1_000_001)
            .read_to_end(&mut source)?;
        if source.len() > 1_000_000 {
            return Err(io::Error::other("Policy exceeds the 1 MB limit"));
        }
        let source = normalize_jsonc(&source)?;
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
        object.insert("version".into(), serde_json::json!(1));
        let parents = match object.remove("extends") {
            None => Vec::new(),
            Some(serde_json::Value::String(parent)) => vec![parent],
            Some(serde_json::Value::Array(parents)) => parents
                .into_iter()
                .map(|parent| {
                    parent.as_str().map(str::to_owned).ok_or_else(|| {
                        io::Error::other("Policy extends entries must be profile names or paths")
                    })
                })
                .collect::<io::Result<Vec<_>>>()?,
            Some(_) => {
                return Err(io::Error::other(
                    "Policy extends must be a profile name, path, or list of them",
                ));
            }
        };
        if resolve_paths {
            resolve_policy_layer_paths(&mut object, &path)?;
        }
        let mut merged = serde_json::Map::new();
        for parent in parents {
            let parent_path = inherited_policy_path(&path, &parent)?;
            let base = load_policy_chain(&parent_path, ancestors, depth + 1, resolve_paths)?;
            merge_policy_objects(&mut merged, base);
        }
        merge_policy_objects(&mut merged, object);
        Ok(merged)
    })();
    ancestors.remove(&path);
    result
}

fn inherited_policy_path(current: &Path, parent: &str) -> io::Result<PathBuf> {
    if parent.is_empty() || parent.contains('\0') {
        return Err(io::Error::other("Policy inheritance entry cannot be empty"));
    }
    let requested = Path::new(parent);
    if requested.is_absolute() || parent.starts_with("./") || parent.starts_with("../") {
        return Ok(if requested.is_absolute() {
            requested.to_owned()
        } else {
            current
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .join(requested)
        });
    }
    if !parent
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    {
        return Err(io::Error::other(
            "Use a profile name or an explicit relative path such as ./base.json in extends",
        ));
    }
    let directory = crate::profiles::profile_directory()?;
    let jsonc = directory.join(format!("{parent}.jsonc"));
    if jsonc.is_file() {
        Ok(jsonc)
    } else {
        Ok(directory.join(format!("{parent}.json")))
    }
}

fn normalize_jsonc(source: &[u8]) -> io::Result<Vec<u8>> {
    let mut uncommented = Vec::with_capacity(source.len());
    let mut index = 0;
    let mut in_string = false;
    let mut escaped = false;
    while index < source.len() {
        let byte = source[index];
        if in_string {
            uncommented.push(byte);
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                in_string = false;
            }
            index += 1;
            continue;
        }
        if byte == b'"' {
            in_string = true;
            uncommented.push(byte);
            index += 1;
        } else if byte == b'/' && source.get(index + 1) == Some(&b'/') {
            index += 2;
            while index < source.len() && !matches!(source[index], b'\n' | b'\r') {
                index += 1;
            }
        } else if byte == b'/' && source.get(index + 1) == Some(&b'*') {
            index += 2;
            let mut closed = false;
            while index < source.len() {
                if source[index] == b'*' && source.get(index + 1) == Some(&b'/') {
                    index += 2;
                    closed = true;
                    break;
                }
                if matches!(source[index], b'\n' | b'\r') {
                    uncommented.push(source[index]);
                }
                index += 1;
            }
            if !closed {
                return Err(io::Error::other("Unterminated JSONC block comment"));
            }
        } else {
            uncommented.push(byte);
            index += 1;
        }
    }
    if in_string {
        return Err(io::Error::other("Unterminated string in JSONC policy"));
    }

    let mut normalized = Vec::with_capacity(uncommented.len());
    index = 0;
    in_string = false;
    escaped = false;
    while index < uncommented.len() {
        let byte = uncommented[index];
        if in_string {
            normalized.push(byte);
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                in_string = false;
            }
            index += 1;
        } else if byte == b'"' {
            in_string = true;
            normalized.push(byte);
            index += 1;
        } else if byte == b',' {
            let mut next = index + 1;
            while next < uncommented.len() && uncommented[next].is_ascii_whitespace() {
                next += 1;
            }
            if !matches!(uncommented.get(next), Some(b'}' | b']')) {
                normalized.push(byte);
            }
            index += 1;
        } else {
            normalized.push(byte);
            index += 1;
        }
    }
    Ok(normalized)
}

fn resolve_policy_layer_paths(
    object: &mut serde_json::Map<String, serde_json::Value>,
    path: &Path,
) -> io::Result<()> {
    let base = path.parent().unwrap_or_else(|| Path::new("."));
    for key in ["read", "write", "write_only"] {
        if let Some(serde_json::Value::Array(paths)) = object.get_mut(key) {
            for value in paths.iter_mut() {
                let Some(path_text) = value.as_str() else {
                    continue;
                };
                let path = Path::new(path_text);
                if path.is_relative() && !path_text.starts_with('$') {
                    *value = serde_json::Value::String(base.join(path).to_string_lossy().into());
                }
            }
        }
    }
    if let Some(serde_json::Value::String(value)) = object.get_mut("cgroup_root") {
        let root = Path::new(value);
        if root.is_relative() {
            *value = base.join(root).to_string_lossy().into_owned();
        }
    }
    Ok(())
}

fn merge_policy_objects(
    target: &mut serde_json::Map<String, serde_json::Value>,
    source: serde_json::Map<String, serde_json::Value>,
) {
    for (key, value) in source {
        match (target.get_mut(&key), value) {
            (Some(serde_json::Value::Array(existing)), serde_json::Value::Array(additions)) => {
                for item in additions {
                    if !existing.contains(&item) {
                        existing.push(item);
                    }
                }
            }
            (Some(serde_json::Value::Object(existing)), serde_json::Value::Object(additions)) => {
                let insensitive = key == "environment"
                    && (existing
                        .get("case_insensitive_vars")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(false)
                        || additions
                            .get("case_insensitive_vars")
                            .and_then(serde_json::Value::as_bool)
                            .unwrap_or(false));
                merge_policy_objects(existing, additions);
                if insensitive {
                    existing.insert("case_insensitive_vars".into(), serde_json::json!(true));
                }
            }
            (_, value) => {
                target.insert(key, value);
            }
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

fn forwarded_for_agent(name: &str, agent: AgentProfile) -> bool {
    if matches!(
        name,
        "PATH"
            | "TERM"
            | "LANG"
            | "LC_ALL"
            | "COLORTERM"
            | "SYSTEMROOT"
            | "SystemRoot"
            | "WINDIR"
            | "HTTP_PROXY"
            | "HTTPS_PROXY"
            | "ALL_PROXY"
            | "NO_PROXY"
            | "http_proxy"
            | "https_proxy"
            | "all_proxy"
            | "no_proxy"
            | "SSL_CERT_FILE"
    ) {
        return true;
    }
    match agent {
        AgentProfile::Codex => matches!(
            name,
            "OPENAI_API_KEY"
                | "OPENAI_BASE_URL"
                | "CODEX_API_KEY"
                | "CODEX_ACCESS_TOKEN"
                | "CODEX_CA_CERTIFICATE"
                | "RUST_LOG"
        ),
        AgentProfile::ClaudeCode => {
            name.starts_with("CLAUDE_")
                || name.starts_with("ANTHROPIC_")
                || name.starts_with("AWS_")
                || name.starts_with("VERTEX_")
        }
        AgentProfile::OpenCode => {
            name.ends_with("_API_KEY")
                || name.ends_with("_AUTH_TOKEN")
                || matches!(
                    name,
                    "AWS_REGION"
                        | "AWS_DEFAULT_REGION"
                        | "AWS_PROFILE"
                        | "AWS_ACCESS_KEY_ID"
                        | "AWS_SECRET_ACCESS_KEY"
                        | "AWS_SESSION_TOKEN"
                        | "GOOGLE_APPLICATION_CREDENTIALS"
                        | "GOOGLE_CLOUD_PROJECT"
                        | "GOOGLE_CLOUD_REGION"
                        | "VERTEX_PROJECT_ID"
                        | "VERTEX_LOCATION"
                )
        }
        AgentProfile::Pi => {
            name.ends_with("_API_KEY")
                || name.ends_with("_AUTH_TOKEN")
                || matches!(
                    name,
                    "GITHUB_TOKEN"
                        | "HF_TOKEN"
                        | "AWS_BEARER_TOKEN_BEDROCK"
                        | "AWS_REGION"
                        | "AWS_DEFAULT_REGION"
                        | "AWS_PROFILE"
                        | "AWS_ACCESS_KEY_ID"
                        | "AWS_SECRET_ACCESS_KEY"
                        | "AWS_SESSION_TOKEN"
                        | "GOOGLE_APPLICATION_CREDENTIALS"
                        | "GOOGLE_CLOUD_PROJECT"
                        | "GOOGLE_CLOUD_REGION"
                        | "VERTEX_PROJECT_ID"
                        | "VERTEX_LOCATION"
                        | "PI_OFFLINE"
                        | "PI_SKIP_VERSION_CHECK"
                        | "PI_TELEMETRY"
                        | "PI_CACHE_RETENTION"
                )
        }
    }
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
