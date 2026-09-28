use serde::Deserialize;
use std::{collections::BTreeMap, io::Read, path::Path};

use super::Issue;

pub const FILES: &[&str] = &[
    ".vscode/mcp.json",
    ".cursor/mcp.json",
    "mcp.json",
    ".mcp.json",
];

#[derive(Clone, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    #[serde(rename = "type", alias = "transport")]
    pub transport: Option<String>,
    pub command: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    pub url: Option<String>,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    #[serde(default)]
    pub disabled: bool,
}
impl Config {
    pub fn kind(&self) -> &'static str {
        if self.command.is_some() {
            "stdio"
        } else if self.url.is_some() {
            "http"
        } else {
            "unknown"
        }
    }
    fn validate(&self) -> Result<(), &'static str> {
        match (self.command.as_deref(), self.url.as_deref()) {
            (Some(command), None) if !command.trim().is_empty() => {
                if self.transport.as_deref().is_none_or(|t| t == "stdio") && self.headers.is_empty()
                {
                    Ok(())
                } else {
                    Err("A command requires stdio transport and no HTTP headers")
                }
            }
            (None, Some(url)) => {
                let url = reqwest::Url::parse(url).map_err(|_| "Invalid MCP URL")?;
                if !matches!(url.scheme(), "http" | "https")
                    || !url.username().is_empty()
                    || url.password().is_some()
                    || url.fragment().is_some()
                {
                    return Err(
                        "Use an HTTP(S) URL without credentials or a fragment; set credentials in headers",
                    );
                }
                if !self.args.is_empty()
                    || !self.env.is_empty()
                    || !self
                        .transport
                        .as_deref()
                        .is_none_or(|t| matches!(t, "http" | "streamable-http" | "streamableHttp"))
                {
                    return Err(
                        "A URL requires Streamable HTTP transport, without command args or env",
                    );
                }
                Ok(())
            }
            _ => Err("Configure exactly one nonempty command or URL"),
        }
    }
}

fn expand(value: &str) -> Result<String, &'static str> {
    let mut rest = value;
    let mut result = String::new();
    while let Some(index) = rest.find("${") {
        result.push_str(&rest[..index]);
        rest = &rest[index + 2..];
        let end = rest
            .find('}')
            .ok_or("Unclosed environment variable placeholder")?;
        let name = rest[..end].strip_prefix("env:").unwrap_or(&rest[..end]);
        if name.is_empty() || !name.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_') {
            return Err("Invalid environment variable placeholder");
        }
        result.push_str(
            &std::env::var(name)
                .map_err(|_| "A referenced backend environment variable is not set")?,
        );
        rest = &rest[end + 1..];
    }
    result.push_str(rest);
    Ok(result)
}

pub struct Discovered {
    pub files: Vec<String>,
    pub servers: BTreeMap<String, (Config, String)>,
    pub issues: Vec<Issue>,
}
pub fn discover(workspace: &Path) -> Discovered {
    let mut found = Discovered {
        files: Vec::new(),
        servers: BTreeMap::new(),
        issues: Vec::new(),
    };
    for name in FILES {
        let path = workspace.join(name);
        if !path.exists() {
            continue;
        }
        found.files.push((*name).into());
        let read = || -> Result<serde_json::Value, &'static str> {
            let resolved = path
                .canonicalize()
                .map_err(|_| "Cannot resolve MCP config")?;
            if !resolved.starts_with(workspace) {
                return Err("MCP config must stay inside its workspace");
            }
            let mut source = String::new();
            std::fs::File::open(resolved)
                .map_err(|_| "Cannot read MCP config")?
                .take(1_000_001)
                .read_to_string(&mut source)
                .map_err(|_| "MCP config must be UTF-8")?;
            if source.len() > 1_000_000 {
                return Err("MCP config exceeds 1 MB");
            }
            serde_json::from_str(&source).map_err(|_| "MCP config must be valid JSON")
        };
        let result = read().and_then(|value| {
            let servers = value
                .get("mcpServers")
                .or_else(|| value.get("servers"))
                .and_then(|v| v.as_object())
                .ok_or("Expected an mcpServers or servers object")?;
            for (server, value) in servers {
                let parse = || -> Result<Config, &'static str> {
                    if server.is_empty() || server.len() > 128 {
                        return Err("Server names must contain 1–128 characters");
                    }
                    let mut config: Config = serde_json::from_value(value.clone())
                        .map_err(|_| "Invalid server configuration fields")?;
                    if !config.disabled {
                        if let Some(command) = &mut config.command {
                            *command = expand(command)?;
                        }
                        if let Some(url) = &mut config.url {
                            *url = expand(url)?;
                        }
                        for value in config
                            .args
                            .iter_mut()
                            .chain(config.env.values_mut())
                            .chain(config.headers.values_mut())
                        {
                            *value = expand(value)?;
                        }
                        config.validate()?;
                    }
                    Ok(config)
                };
                // Higher-priority entries replace lower ones even when invalid.
                found.servers.remove(server);
                match parse() {
                    Ok(config) => {
                        found
                            .servers
                            .insert(server.clone(), (config, (*name).into()));
                    }
                    Err(message) => found.issues.push(Issue {
                        path: format!("{name}: {server}"),
                        message: message.into(),
                    }),
                }
            }
            Ok(())
        });
        if let Err(message) = result {
            found.issues.push(Issue {
                path: (*name).into(),
                message: message.into(),
            });
        }
    }
    if found.servers.len() > 32 {
        found.servers = found.servers.into_iter().take(32).collect();
        found.issues.push(Issue {
            path: "MCP".into(),
            message: "Only the first 32 servers are loaded".into(),
        });
    }
    found
}
