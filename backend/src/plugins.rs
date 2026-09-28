//! Workspace Agent Plugins 1.0.0. Discovery never executes plugin code; only
//! connected MCP servers are started by the MCP manager.
use crate::{mcp::config::Config, skills::Skill};
use serde::Serialize;
use serde_json::{Map, Value};
use std::{
    collections::{BTreeMap, BTreeSet, hash_map::DefaultHasher},
    fs,
    hash::{Hash, Hasher},
    io::Read,
    path::{Path, PathBuf},
};

const MANIFEST_SCHEMA: &str = "https://agent-plugins.org/schemas/1.0.0/plugin.schema.json";
const MCP_SCHEMA: &str = "https://agent-plugins.org/schemas/1.0.0/mcp.schema.json";
const LIMIT: u64 = 1_000_000;

#[derive(Clone, Default, PartialEq, Serialize)]
pub struct Catalog {
    pub directory: String,
    pub items: Vec<Plugin>,
    pub issues: Vec<Issue>,
}
#[derive(Clone, PartialEq, Serialize)]
pub struct Plugin {
    pub name: String,
    pub version: Option<String>,
    pub description: Option<String>,
    pub path: String,
    pub skills: Vec<String>,
    pub mcp_servers: Vec<String>,
    pub issues: Vec<Issue>,
}
#[derive(Clone, PartialEq, Serialize)]
pub struct Issue {
    pub path: String,
    pub message: String,
}
pub struct Discovered {
    pub catalog: Catalog,
    pub skills: Vec<(Skill, PathBuf)>,
    pub servers: Vec<(String, Config, String)>,
    pub fingerprint: u64,
}
fn issue(path: impl Into<String>, message: impl Into<String>) -> Issue {
    Issue {
        path: path.into(),
        message: message.into(),
    }
}
fn read(path: &Path, root: &Path) -> Result<String, String> {
    let resolved = path.canonicalize().map_err(|e| e.to_string())?;
    if !resolved.starts_with(root) || !resolved.is_file() {
        return Err("Package file must be a regular file inside the plugin".into());
    }
    let mut source = String::new();
    fs::File::open(resolved)
        .map_err(|e| e.to_string())?
        .take(LIMIT + 1)
        .read_to_string(&mut source)
        .map_err(|_| "Package file must be UTF-8".to_owned())?;
    if source.len() as u64 > LIMIT {
        return Err("Package file exceeds 1 MB".into());
    }
    Ok(source)
}
fn object<'a>(value: &'a Value, label: &str) -> Result<&'a Map<String, Value>, String> {
    value
        .as_object()
        .ok_or_else(|| format!("{label} must be an object"))
}
fn string_field<'a>(object: &'a Map<String, Value>, key: &str) -> Result<Option<&'a str>, String> {
    object
        .get(key)
        .map(|v| v.as_str().ok_or_else(|| format!("{key} must be a string")))
        .transpose()
}
fn manifest(value: &Value, path: &str) -> Result<Plugin, String> {
    let fields = object(value, "plugin.json")?;
    if string_field(fields, "$schema")? != Some(MANIFEST_SCHEMA) {
        return Err("Unsupported or missing Agent Plugins $schema".into());
    }
    let name = string_field(fields, "name")?.ok_or("Missing plugin name")?;
    let bytes = name.as_bytes();
    if bytes.is_empty()
        || bytes.len() > 64
        || !bytes[0].is_ascii_alphanumeric()
        || !bytes[bytes.len() - 1].is_ascii_alphanumeric()
        || bytes
            .iter()
            .any(|c| !(c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'-' || *c == b'.'))
        || name.contains("--")
        || name.contains("..")
    {
        return Err("Invalid plugin name".into());
    }
    for key in [
        "version",
        "description",
        "homepage",
        "repository",
        "license",
    ] {
        string_field(fields, key)?;
    }
    if let Some(author) = fields.get("author") {
        let author = object(author, "author")?;
        for (key, value) in author {
            if !matches!(key.as_str(), "name" | "email" | "url") || !value.is_string() {
                return Err("author may contain only name, email, and url strings".into());
            }
        }
    }
    if let Some(keywords) = fields.get("keywords")
        && !keywords
            .as_array()
            .is_some_and(|a| a.iter().all(Value::is_string))
    {
        return Err("keywords must be an array of strings".into());
    }
    let mut issues = Vec::new();
    for key in fields.keys() {
        if !matches!(
            key.as_str(),
            "$schema"
                | "name"
                | "version"
                | "description"
                | "author"
                | "homepage"
                | "repository"
                | "license"
                | "keywords"
                | "extensions"
        ) {
            issues.push(issue(path, format!("Unknown manifest field {key} ignored")));
        }
    }
    if fields.get("extensions").is_some_and(|v| !v.is_object()) {
        issues.push(issue(path, "Non-object extensions ignored"));
    }
    Ok(Plugin {
        name: name.into(),
        version: string_field(fields, "version")?.map(str::to_owned),
        description: string_field(fields, "description")?.map(str::to_owned),
        path: path.into(),
        skills: Vec::new(),
        mcp_servers: Vec::new(),
        issues,
    })
}
fn relative(root: &Path, value: &str) -> Result<PathBuf, String> {
    if !value.starts_with("./") {
        return Err("Path must begin with ./".into());
    }
    let path = root
        .join(&value[2..])
        .canonicalize()
        .map_err(|e| format!("Cannot resolve plugin path: {e}"))?;
    if !path.starts_with(root) {
        return Err("Plugin path escapes its root".into());
    }
    Ok(path)
}
fn expand(value: &str, root: &Path, data: &Path) -> String {
    let mut rest = value;
    let mut result = String::new();
    while let Some(index) = rest.find("${") {
        result.push_str(&rest[..index]);
        rest = &rest[index..];
        if let Some(after) = rest.strip_prefix("${PLUGIN_ROOT}") {
            result.push_str(&root.to_string_lossy());
            rest = after;
        } else if let Some(after) = rest.strip_prefix("${PLUGIN_DATA}") {
            result.push_str(&data.to_string_lossy());
            rest = after;
        } else {
            result.push_str("${");
            rest = &rest[2..];
        }
    }
    result.push_str(rest);
    result
}
fn string_map(value: Option<&Value>, label: &str) -> Result<BTreeMap<String, String>, String> {
    let Some(value) = value else {
        return Ok(BTreeMap::new());
    };
    object(value, label)?
        .iter()
        .map(|(k, v)| {
            v.as_str()
                .map(|s| (k.clone(), s.to_owned()))
                .ok_or_else(|| format!("{label} values must be strings"))
        })
        .collect()
}
fn parse_server(value: &Value, root: &Path, data: &Path) -> Result<Config, String> {
    let fields = object(value, "MCP server")?;
    let kind = string_field(fields, "type")?.ok_or("MCP server type is required")?;
    let valid = match kind {
        "stdio" => &["type", "command", "args", "env", "cwd"][..],
        "streamable-http" => &["type", "url", "headers"][..],
        "sse" => return Err("SSE transport is not supported; use streamable-http".into()),
        _ => return Err("Unsupported MCP transport".into()),
    };
    if fields.keys().any(|key| !valid.contains(&key.as_str())) {
        return Err("Unknown MCP server field".into());
    }
    let mut config = Config {
        transport: Some(kind.into()),
        ..Config::default()
    };
    if kind == "stdio" {
        let command = string_field(fields, "command")?.ok_or("stdio command is required")?;
        if command.is_empty() {
            return Err("stdio command cannot be empty".into());
        }
        config.command = Some(if command.starts_with("./") {
            let path = relative(root, command)?;
            if !path.is_file() {
                return Err("stdio command must be a file".into());
            }
            path.to_string_lossy().into_owned()
        } else if command.contains('/')
            || command.contains('\\')
            || command.starts_with('.')
            || Path::new(command).is_absolute()
        {
            return Err("stdio command must be a bare executable or ./ plugin path".into());
        } else {
            command.into()
        });
        config.args = fields
            .get("args")
            .map(|v| {
                v.as_array()
                    .ok_or("args must be a string array")
                    .and_then(|a| {
                        a.iter()
                            .map(|v| {
                                v.as_str()
                                    .map(str::to_owned)
                                    .ok_or("args must be a string array")
                            })
                            .collect()
                    })
            })
            .transpose()?
            .unwrap_or_default();
        config.args = config.args.iter().map(|v| expand(v, root, data)).collect();
        config.env = string_map(fields.get("env"), "env")?;
        if config
            .env
            .keys()
            .any(|k| k.eq_ignore_ascii_case("PLUGIN_ROOT") || k.eq_ignore_ascii_case("PLUGIN_DATA"))
        {
            return Err("PLUGIN_ROOT and PLUGIN_DATA are reserved".into());
        }
        config.env = config
            .env
            .into_iter()
            .map(|(k, v)| (k, expand(&v, root, data)))
            .collect();
        let cwd = string_field(fields, "cwd")?;
        config.cwd = Some(match cwd {
            None => root.to_owned(),
            Some(v) if v.starts_with("./") => relative(root, v)?,
            Some("${PLUGIN_ROOT}") => root.to_owned(),
            Some(v) if v.starts_with("${PLUGIN_ROOT}/") => {
                relative(root, &format!("./{}", &v[15..]))?
            }
            Some("${PLUGIN_DATA}") => data.to_owned(),
            Some(v) if v.starts_with("${PLUGIN_DATA}/") => {
                relative(data, &format!("./{}", &v[15..]))?
            }
            Some(_) => return Err("cwd must be relative to PLUGIN_ROOT or PLUGIN_DATA".into()),
        });
        if !config.cwd.as_ref().unwrap().is_dir() {
            return Err("cwd must be a directory".into());
        }
        config.plugin_root = Some(root.to_owned());
        config.plugin_data = Some(data.to_owned());
    } else {
        let url = string_field(fields, "url")?.ok_or("Streamable HTTP url is required")?;
        let url = reqwest::Url::parse(url).map_err(|_| "Invalid MCP URL")?;
        if !matches!(url.scheme(), "http" | "https")
            || !url.username().is_empty()
            || url.password().is_some()
            || url.fragment().is_some()
        {
            return Err("MCP URL must be HTTP(S) without credentials or fragment".into());
        }
        let host = url.host_str().unwrap_or_default();
        let ip = host.trim_start_matches('[').trim_end_matches(']');
        let loopback = host.eq_ignore_ascii_case("localhost")
            || ip
                .parse::<std::net::IpAddr>()
                .is_ok_and(|ip| ip.is_loopback());
        if url.scheme() != "https" && !loopback {
            return Err("Remote MCP URL must use HTTPS".into());
        }
        config.url = Some(url.into());
        let headers = string_map(fields.get("headers"), "headers")?;
        let mut names = BTreeSet::new();
        for (name, value) in &headers {
            if !names.insert(name.to_ascii_lowercase())
                || reqwest::header::HeaderName::from_bytes(name.as_bytes()).is_err()
                || reqwest::header::HeaderValue::from_str(value).is_err()
            {
                return Err("Invalid or duplicate HTTP header".into());
            }
        }
        config.headers = headers;
    }
    Ok(config)
}
pub fn discover(workspace: &Path) -> Discovered {
    let directory = workspace.join(".agents/plugins");
    let mut result = Discovered {
        catalog: Catalog {
            directory: ".agents/plugins/".into(),
            ..Catalog::default()
        },
        skills: Vec::new(),
        servers: Vec::new(),
        fingerprint: 0,
    };
    let mut hash = DefaultHasher::new();
    let Ok(entries) = fs::read_dir(&directory) else {
        return result;
    };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    let mut names = BTreeSet::new();
    for entry in entries.into_iter().take(64) {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let label = format!(".agents/plugins/{}", entry.file_name().to_string_lossy());
        let Ok(root) = path.canonicalize() else {
            continue;
        };
        let manifest_path = format!("{label}/plugin.json");
        let manifest_source = match read(&root.join("plugin.json"), &root) {
            Ok(source) => source,
            Err(message) => {
                result.catalog.issues.push(issue(manifest_path, message));
                continue;
            }
        };
        manifest_source.hash(&mut hash);
        let mut plugin = match serde_json::from_str::<Value>(&manifest_source)
            .map_err(|e| e.to_string())
            .and_then(|v| manifest(&v, &manifest_path))
        {
            Ok(plugin) => plugin,
            Err(message) => {
                result.catalog.issues.push(issue(manifest_path, message));
                continue;
            }
        };
        if !names.insert(plugin.name.clone()) {
            result
                .catalog
                .issues
                .push(issue(&label, "Duplicate plugin name"));
            continue;
        }
        let skills_path = root.join("skills");
        if fs::symlink_metadata(&skills_path).is_ok() {
            match skills_path.canonicalize() {
                Ok(dir) if dir.starts_with(&root) && dir.is_dir() => {
                    if let Ok(entries) = fs::read_dir(dir) {
                        let mut entries: Vec<_> = entries.flatten().collect();
                        entries.sort_by_key(|e| e.file_name());
                        for skill in entries {
                            if !skill.path().is_dir() {
                                continue;
                            }
                            let name = skill.file_name().to_string_lossy().into_owned();
                            let skill_path = format!("{label}/skills/{name}/SKILL.md");
                            let found = (|| -> Result<(Skill, PathBuf), String> {
                                let folder =
                                    skill.path().canonicalize().map_err(|e| e.to_string())?;
                                if !folder.starts_with(&root) {
                                    return Err("Skill escapes plugin root".into());
                                }
                                let source = read(&folder.join("SKILL.md"), &root)?;
                                source.hash(&mut hash);
                                Ok((
                                    crate::skills::parse(&source, &name, skill_path.clone())?,
                                    folder,
                                ))
                            })();
                            match found {
                                Ok((item, folder)) => {
                                    plugin.skills.push(name);
                                    result.skills.push((item, folder));
                                }
                                Err(message) => plugin.issues.push(issue(skill_path, message)),
                            }
                        }
                    }
                }
                _ => plugin.issues.push(issue(
                    format!("{label}/skills"),
                    "Skills directory must stay inside plugin root",
                )),
            }
        }
        let mcp_path = root.join("mcp.json");
        if fs::symlink_metadata(&mcp_path).is_ok() {
            let mcp_label = format!("{label}/mcp.json");
            let parsed = read(&mcp_path, &root)
                .and_then(|source| {
                    source.hash(&mut hash);
                    serde_json::from_str::<Value>(&source).map_err(|e| e.to_string())
                })
                .and_then(|value| {
                    let fields = object(&value, "mcp.json")?;
                    if fields.len() != 2 || string_field(fields, "$schema")? != Some(MCP_SCHEMA) {
                        return Err("Invalid Agent Plugins MCP schema or top-level fields".into());
                    }
                    let servers = object(
                        fields.get("mcpServers").ok_or("Missing mcpServers")?,
                        "mcpServers",
                    )?;
                    Ok(servers.clone())
                });
            match parsed {
                Ok(servers) => {
                    let data = workspace.join(".solmu/plugin-data").join(entry.file_name());
                    for (name, value) in servers {
                        let source = format!("{mcp_label}: {name}");
                        if name.is_empty() || name.len() > 128 {
                            plugin.issues.push(issue(source, "Invalid MCP server name"));
                            continue;
                        }
                        if let Err(e) = fs::create_dir_all(&data) {
                            plugin
                                .issues
                                .push(issue(source, format!("Cannot create plugin data: {e}")));
                            continue;
                        }
                        let Ok(data) = data.canonicalize() else {
                            plugin
                                .issues
                                .push(issue(source, "Cannot resolve plugin data"));
                            continue;
                        };
                        if !data.starts_with(workspace) {
                            plugin
                                .issues
                                .push(issue(source, "Plugin data must stay inside workspace"));
                            continue;
                        }
                        match parse_server(&value, &root, &data) {
                            Ok(config) => {
                                plugin.mcp_servers.push(name.clone());
                                result.servers.push((
                                    format!("plugin::{}::{name}", plugin.name),
                                    config,
                                    mcp_label.clone(),
                                ));
                            }
                            Err(message) => plugin.issues.push(issue(source, message)),
                        }
                    }
                }
                Err(message) => plugin.issues.push(issue(mcp_label, message)),
            }
        }
        for problem in &plugin.issues {
            problem.path.hash(&mut hash);
            problem.message.hash(&mut hash);
        }
        result.catalog.items.push(plugin);
    }
    result.fingerprint = hash.finish();
    result
}
