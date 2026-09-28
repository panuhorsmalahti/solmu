pub(crate) mod config;
mod connection;

use crate::{
    api::state::Change,
    tools::{AgentTool, Context},
};
use futures_util::{StreamExt, future::BoxFuture};
use genai::chat::Tool;
use serde::Serialize;
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    hash::{Hash, Hasher},
    path::PathBuf,
    sync::{Arc, atomic::Ordering},
    time::{Duration, Instant},
};
use tokio::sync::{Mutex, broadcast};

#[derive(Clone, Serialize, PartialEq)]
pub struct Issue {
    pub path: String,
    pub message: String,
}
#[derive(Clone, Serialize, PartialEq)]
pub struct ToolInfo {
    pub name: String,
    pub agent_name: String,
    pub description: String,
}
#[derive(Clone, Serialize, PartialEq)]
pub struct Server {
    pub name: String,
    pub source: String,
    pub transport: String,
    pub status: String,
    pub protocol_version: Option<String>,
    pub tools: Vec<ToolInfo>,
    pub error: Option<String>,
}
#[derive(Clone, Default, Serialize, PartialEq)]
pub struct Catalog {
    pub workspace: String,
    pub files: Vec<String>,
    pub servers: Vec<Server>,
    pub issues: Vec<Issue>,
}
#[derive(Clone, Default)]
pub struct Snapshot {
    pub catalog: Catalog,
    pub tools: Vec<Arc<dyn AgentTool>>,
    pub plugins: crate::plugins::Catalog,
}
struct Connected {
    config: config::Config,
    source: String,
    client: Option<Arc<connection::Client>>,
    status: Server,
    tools: Vec<Arc<dyn AgentTool>>,
    checked: Instant,
}
impl Drop for Connected {
    fn drop(&mut self) {
        if let Some(client) = &self.client {
            client.cancellation_token().cancel();
        }
    }
}
impl Connected {
    async fn refresh(&mut self, workspace: &std::path::Path) {
        if self.config.disabled {
            return;
        }
        if self
            .client
            .as_ref()
            .is_some_and(|client| client.is_closed())
        {
            self.client = None;
            self.tools.clear();
            self.status.tools.clear();
            self.status.status = "error".into();
            self.status.error = Some("MCP server disconnected; reconnecting automatically".into());
        }
        let dirty = self
            .client
            .as_ref()
            .is_some_and(|client| client.service().dirty.swap(false, Ordering::AcqRel));
        if !dirty && self.checked.elapsed() < Duration::from_secs(5) {
            return;
        }
        self.checked = Instant::now();
        let result = tokio::time::timeout(Duration::from_secs(8), async {
            if self.client.is_none() {
                self.client = Some(connection::connect(&self.config, workspace).await?);
            }
            let client = self.client.as_ref().unwrap().clone();
            let definitions = if client
                .peer_info()
                .is_some_and(|info| info.capabilities.tools.is_some())
            {
                client
                    .list_all_tools()
                    .await
                    .map_err(|_| "Cannot list MCP tools; reconnecting automatically")?
            } else {
                Vec::new()
            };
            if definitions.len() > 256 {
                return Err("MCP server exposes more than 256 tools".into());
            }
            let mut tools: Vec<Arc<dyn AgentTool>> = Vec::new();
            let mut info = Vec::new();
            let mut names = BTreeSet::new();
            for definition in definitions {
                if definition.name.is_empty() || !names.insert(definition.name.to_string()) {
                    return Err("MCP tool names must be nonempty and unique".into());
                }
                let agent_name = tool_name(&self.status.name, &definition.name);
                let description = definition
                    .description
                    .as_deref()
                    .unwrap_or("MCP tool")
                    .to_owned();
                let tool = Tool::new(&agent_name)
                    .with_description(&description)
                    .with_schema(Value::Object((*definition.input_schema).clone()));
                info.push(ToolInfo {
                    name: definition.name.to_string(),
                    agent_name,
                    description,
                });
                tools.push(Arc::new(RemoteTool {
                    definition: tool,
                    remote_name: definition.name.to_string(),
                    client: client.clone(),
                }));
            }
            self.tools = tools;
            self.status.tools = info;
            self.status.protocol_version = client
                .peer_info()
                .map(|info| info.protocol_version.to_string());
            Ok::<(), String>(())
        })
        .await
        .unwrap_or_else(|_| {
            Err("MCP connection or discovery timed out; retrying automatically".into())
        });
        match result {
            Ok(()) => {
                self.status.status = "connected".into();
                self.status.error = None;
            }
            Err(message) => {
                if let Some(client) = self.client.take() {
                    client.cancellation_token().cancel();
                }
                self.tools.clear();
                self.status.tools.clear();
                self.status.status = "error".into();
                self.status.error = Some(message);
            }
        }
    }
}
fn tool_name(server: &str, tool: &str) -> String {
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    (server, tool).hash(&mut hash);
    let slug = |value: &str| {
        value
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .take(16)
            .collect::<String>()
    };
    format!("mcp_{}_{}_{:016x}", slug(server), slug(tool), hash.finish())
}
struct RemoteTool {
    definition: Tool,
    remote_name: String,
    client: Arc<connection::Client>,
}
impl AgentTool for RemoteTool {
    fn definition(&self) -> Tool {
        self.definition.clone()
    }
    fn execute(
        &self,
        arguments: Value,
        context: Context,
    ) -> BoxFuture<'static, Result<Value, String>> {
        let client = self.client.clone();
        let name = self.remote_name.clone();
        Box::pin(async move {
            let arguments = arguments
                .as_object()
                .cloned()
                .ok_or("MCP arguments must be an object")?;
            let params = rmcp::model::CallToolRequestParams::new(name).with_arguments(arguments);
            // Cancel the actual request handle, so stdio receives a cancellation
            // notification and HTTP closes the response stream. Never retry a call.
            let mut handle = client
                .send_cancellable_request(
                    rmcp::model::ClientRequest::CallToolRequest(rmcp::model::CallToolRequest::new(
                        params,
                    )),
                    Default::default(),
                )
                .await
                .map_err(|_| "MCP tool request failed")?;
            let response = tokio::select! {
                response = &mut handle.rx => response.map_err(|_| "MCP server disconnected")?.map_err(|_| "MCP tool request failed")?,
                _ = context.cancellation.cancelled() => { let _ = handle.cancel(Some("Solmu response stopped".into())).await; return Err("Tool cancelled".into()); },
                _ = tokio::time::sleep(Duration::from_secs(60)) => { let _ = handle.cancel(Some("Solmu tool timeout".into())).await; return Err("MCP tool timed out after 60 seconds".into()); },
            };
            let mut result =
                serde_json::to_value(response).map_err(|_| "Invalid MCP tool result")?;
            if result["resultType"] == "input_required" {
                return Err(
                    "MCP server requested interactive input, which Solmu does not advertise".into(),
                );
            }
            if result["resultType"] == "task" {
                return Err("MCP tasks are not enabled".into());
            }
            if let Some(result) = result.as_object_mut() {
                result.remove("_meta");
            }
            if serde_json::to_vec(&result)
                .map_err(|_| "Invalid MCP result")?
                .len()
                > 1_000_000
            {
                return Err("MCP tool result exceeds 1 MB".into());
            }
            Ok(result)
        })
    }
}
struct Watched {
    threads: BTreeSet<String>,
    servers: BTreeMap<String, Connected>,
    snapshot: Snapshot,
}
type Workspaces = Arc<Mutex<BTreeMap<PathBuf, Arc<Mutex<Watched>>>>>;
#[derive(Clone)]
pub struct Manager {
    watched: Workspaces,
    events: broadcast::Sender<Change>,
}
impl Manager {
    pub fn new(events: broadcast::Sender<Change>) -> Self {
        let watched: Workspaces = Arc::default();
        let weak = Arc::downgrade(&watched);
        let changes = events.clone();
        tokio::spawn(async move {
            let mut timer = tokio::time::interval(Duration::from_secs(1));
            timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                timer.tick().await;
                let Some(watched) = weak.upgrade() else {
                    return;
                };
                let entries: Vec<_> = watched
                    .lock()
                    .await
                    .iter()
                    .map(|(path, entry)| (path.clone(), entry.clone()))
                    .collect();
                drop(watched);
                futures_util::stream::iter(entries)
                    .for_each_concurrent(8, |(path, entry)| {
                        let changes = changes.clone();
                        async move {
                            refresh(path, &mut *entry.lock().await, &changes).await;
                        }
                    })
                    .await;
            }
        });
        Self { watched, events }
    }
    pub async fn load(&self, thread: &str, workspace: PathBuf) -> Snapshot {
        let entry = self
            .watched
            .lock()
            .await
            .entry(workspace.clone())
            .or_insert_with(|| {
                Arc::new(Mutex::new(Watched {
                    threads: BTreeSet::new(),
                    servers: BTreeMap::new(),
                    snapshot: Snapshot::default(),
                }))
            })
            .clone();
        let mut entry = entry.lock().await;
        entry.threads.insert(thread.into());
        refresh(workspace, &mut entry, &self.events).await;
        entry.snapshot.clone()
    }
    pub async fn forget(&self, thread: &str) {
        let mut watched = self.watched.lock().await;
        let mut empty = Vec::new();
        for (path, entry) in watched.iter() {
            let mut entry = entry.lock().await;
            entry.threads.remove(thread);
            if entry.threads.is_empty() {
                empty.push(path.clone());
            }
        }
        for path in empty {
            watched.remove(&path);
        }
    }
}
async fn refresh(workspace: PathBuf, watched: &mut Watched, changes: &broadcast::Sender<Change>) {
    let path = workspace.clone();
    let (found, plugins) = tokio::task::spawn_blocking(move || {
        let mut found = config::discover(&path);
        let plugins = crate::plugins::discover(&path);
        for (name, config, source) in &plugins.servers {
            found
                .servers
                .insert(name.clone(), (config.clone(), source.clone()));
        }
        (found, plugins)
    })
    .await
    .expect("MCP discovery task");
    watched.servers.retain(|name, entry| {
        found
            .servers
            .get(name)
            .is_some_and(|(config, source)| config == &entry.config && source == &entry.source)
    });
    for (name, (config, source)) in found.servers {
        watched
            .servers
            .entry(name.clone())
            .or_insert_with(|| Connected {
                status: Server {
                    name,
                    source: source.clone(),
                    transport: config.kind().into(),
                    status: if config.disabled {
                        "disabled"
                    } else {
                        "connecting"
                    }
                    .into(),
                    protocol_version: None,
                    tools: Vec::new(),
                    error: None,
                },
                config,
                source,
                client: None,
                tools: Vec::new(),
                checked: Instant::now() - Duration::from_secs(60),
            });
    }
    futures_util::future::join_all(
        watched
            .servers
            .values_mut()
            .map(|entry| entry.refresh(&workspace)),
    )
    .await;
    let snapshot = Snapshot {
        catalog: Catalog {
            workspace: workspace.to_string_lossy().into_owned(),
            files: found.files,
            servers: watched
                .servers
                .values()
                .map(|entry| entry.status.clone())
                .collect(),
            issues: found.issues,
        },
        tools: watched
            .servers
            .values()
            .flat_map(|entry| entry.tools.clone())
            .collect(),
        plugins: plugins.catalog,
    };
    if snapshot.plugins != watched.snapshot.plugins {
        for thread in &watched.threads {
            let _ = changes.send(Change::plugins(thread));
        }
    }
    if snapshot.catalog != watched.snapshot.catalog {
        for thread in &watched.threads {
            let _ = changes.send(Change::mcp(thread));
        }
    }
    watched.snapshot = snapshot;
}
