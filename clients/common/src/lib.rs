use eventsource_stream::Eventsource;
use futures_util::{Stream, StreamExt};
use reqwest::{Client, Method, Response};
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::{pin::Pin, time::Duration};

pub mod automation;

#[derive(Debug, Clone, Deserialize)]
pub struct Thread {
    pub id: String,
    pub title: String,
    pub model: Option<String>,
    pub workspace: Option<String>,
}
#[derive(Debug, Clone, Deserialize)]
pub struct Profile {
    pub system_prompt: String,
    pub model: Option<String>,
    pub backend_default_model: Option<String>,
    pub edited_at: String,
}
#[derive(Debug, Clone, Deserialize)]
pub struct ModelCatalog {
    pub provider: Option<String>,
    pub default_model: Option<String>,
    pub models: Vec<Model>,
}
#[derive(Debug, Clone, Deserialize)]
pub struct Model {
    pub id: String,
    pub name: String,
}
#[derive(Debug, Clone, Deserialize)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub path: String,
    pub compatibility: Option<String>,
}
#[derive(Debug, Clone, Deserialize)]
pub struct SkillIssue {
    pub path: String,
    pub message: String,
}
#[derive(Debug, Clone, Default, Deserialize)]
pub struct SkillCatalog {
    pub directory: String,
    pub items: Vec<Skill>,
    pub issues: Vec<SkillIssue>,
}
#[derive(Debug, Clone, Default, Deserialize)]
pub struct McpCatalog {
    pub workspace: String,
    pub files: Vec<String>,
    pub servers: Vec<McpServer>,
    pub issues: Vec<SkillIssue>,
}
#[derive(Debug, Clone, Deserialize)]
pub struct McpServer {
    pub name: String,
    pub source: String,
    pub transport: String,
    pub status: String,
    pub protocol_version: Option<String>,
    pub tools: Vec<McpTool>,
    pub error: Option<String>,
}
#[derive(Debug, Clone, Deserialize)]
pub struct McpTool {
    pub name: String,
    pub agent_name: String,
    pub description: String,
}
#[derive(Debug, Clone, Default, Deserialize)]
pub struct PluginCatalog {
    pub directory: String,
    pub items: Vec<Plugin>,
    pub issues: Vec<SkillIssue>,
}
#[derive(Debug, Clone, Deserialize)]
pub struct Plugin {
    pub name: String,
    pub version: Option<String>,
    pub description: Option<String>,
    pub path: String,
    pub skills: Vec<String>,
    pub mcp_servers: Vec<String>,
    pub issues: Vec<SkillIssue>,
}
impl PluginCatalog {
    pub fn text(&self) -> String {
        let mut lines = vec![
            format!(
                "{} {} installed",
                self.items.len(),
                if self.items.len() == 1 {
                    "plugin"
                } else {
                    "plugins"
                }
            ),
            String::new(),
        ];
        if self.items.is_empty() {
            lines.push("No plugins installed in this workspace.".into());
        }
        for plugin in &self.items {
            lines.push(format!(
                "{}{}",
                plugin.name,
                plugin
                    .version
                    .as_ref()
                    .map_or(String::new(), |v| format!(" · {v}"))
            ));
            if let Some(description) = &plugin.description {
                lines.push(description.clone());
            }
            lines.push(format!(
                "{} · {} {} · {} MCP {}",
                plugin.path,
                plugin.skills.len(),
                if plugin.skills.len() == 1 {
                    "skill"
                } else {
                    "skills"
                },
                plugin.mcp_servers.len(),
                if plugin.mcp_servers.len() == 1 {
                    "server"
                } else {
                    "servers"
                }
            ));
            for issue in &plugin.issues {
                lines.push(format!("Not loaded: {}\n{}", issue.path, issue.message));
            }
            lines.push(String::new());
        }
        for issue in &self.issues {
            lines.push(format!("Not loaded: {}\n{}", issue.path, issue.message));
        }
        lines.push("Install plugin folders in .agents/plugins/; each needs plugin.json.".into());
        lines.join("\n")
    }
}
impl McpCatalog {
    pub fn text(&self) -> String {
        let mut lines = vec![
            format!("{} MCP servers configured", self.servers.len()),
            String::new(),
        ];
        if self.servers.is_empty() {
            lines.push("No MCP servers configured in this workspace.".into());
        }
        for server in &self.servers {
            lines.push(format!(
                "{} · {} · {}",
                server.name, server.status, server.transport
            ));
            lines.push(format!(
                "{} · {} tools · protocol {}",
                server.source,
                server.tools.len(),
                server
                    .protocol_version
                    .as_deref()
                    .unwrap_or("not connected")
            ));
            if let Some(error) = &server.error {
                lines.push(error.clone());
            }
            for tool in &server.tools {
                lines.push(format!(
                    "  {} — {}\n  {}",
                    tool.name, tool.description, tool.agent_name
                ));
            }
            lines.push(String::new());
        }
        for issue in &self.issues {
            lines.push(format!("Not loaded: {}\n{}", issue.path, issue.message));
        }
        lines.push(
            "Configure servers in .mcp.json or mcp.json. Changes apply automatically.".into(),
        );
        lines.join("\n")
    }
}
impl SkillCatalog {
    pub fn text(&self) -> String {
        let mut lines = vec![
            format!(
                "{} {} discovered",
                self.items.len(),
                if self.items.len() == 1 {
                    "skill"
                } else {
                    "skills"
                }
            ),
            String::new(),
        ];
        if self.items.is_empty() {
            lines.push("No skills installed in this workspace.".into());
        }
        for skill in &self.items {
            lines.extend([
                skill.name.clone(),
                skill.description.clone(),
                skill.path.clone(),
            ]);
            if let Some(compatibility) = &skill.compatibility {
                lines.push(format!("Requires: {compatibility}"));
            }
            lines.push(String::new());
        }
        for issue in &self.issues {
            lines.push(format!("Not loaded: {}\n{}\n", issue.path, issue.message));
        }
        lines.push("Install skill folders in .agents/skills/; each needs SKILL.md.".into());
        lines.join("\n")
    }
}
#[derive(Debug, Clone, Deserialize)]
pub struct Message {
    pub id: String,
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ToolRun {
    pub id: String,
    pub message_id: String,
    pub name: String,
    pub arguments: Value,
    pub status: String,
    pub result: Option<Value>,
}
#[derive(Debug, Clone, Deserialize)]
pub struct AuditRun {
    pub id: String,
    pub sequence: i64,
    pub thread_id: String,
    pub thread_title: String,
    pub message_id: String,
    pub call_id: String,
    pub name: String,
    pub arguments: Value,
    pub status: String,
    pub result: Option<Value>,
    pub created_at: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
}
#[derive(Debug, Clone, Deserialize)]
pub struct AuditPage {
    pub items: Vec<AuditRun>,
    pub next_cursor: Option<i64>,
}
#[derive(Debug, Clone, Deserialize)]
pub struct ScheduledTask {
    pub id: String,
    pub name: String,
    pub prompt: String,
    pub schedule_kind: String,
    pub schedule: String,
    pub thread_id: String,
    pub enabled: bool,
    pub running: bool,
    pub next_run_at: Option<String>,
    pub last_run_at: Option<String>,
    pub last_status: Option<String>,
}
#[derive(Debug, Clone, Deserialize)]
pub struct TaskRun {
    pub id: String,
    pub scheduled_for: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub status: String,
    pub error: Option<String>,
}
impl ToolRun {
    pub fn details(&self) -> String {
        let arguments = format!("Arguments: {}", self.arguments);
        match &self.result {
            Some(result) => format!("{arguments}\nResult: {result}"),
            None => arguments,
        }
    }
}

#[derive(Clone)]
pub struct Api {
    client: Client,
    base: String,
    workspace: Option<String>,
}

struct RequestFailure {
    code: Option<String>,
    message: String,
}

impl Api {
    pub fn base(&self) -> String {
        self.base.clone()
    }
    pub fn changes(&self) -> Pin<Box<dyn Stream<Item = Connection> + Send>> {
        let url = format!("{}/api/v1/events", self.base.replacen("http", "ws", 1));
        Box::pin(async_stream::stream! {
            loop {
                if let Ok((mut socket, _)) = tokio_tungstenite::connect_async(&url).await {
                    yield Connection::Connected;
                    yield Connection::ProfileChanged;
                    while let Some(Ok(message)) = socket.next().await {
                        if message.is_close() { break; }
                        if let Ok(text) = message.to_text() && let Ok(event) = serde_json::from_str::<Value>(text) {
                            match event["type"].as_str() {
                                Some("conversation_changed" | "skills_changed" | "mcp_changed" | "plugins_changed") => {
                                    yield Connection::Changed;
                                    if event["thread_id"].is_null() { yield Connection::ProfileChanged; }
                                },
                                Some("profile_changed") => yield Connection::ProfileChanged,
                                Some("tasks_changed") => yield Connection::TasksChanged,
                                _ => {},
                            }
                        }
                    }
                }
                yield Connection::Disconnected;
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        })
    }
    pub fn new(base: impl Into<String>) -> Self {
        Self {
            client: Client::builder()
                .connect_timeout(Duration::from_secs(10))
                .build()
                .expect("HTTP client"),
            base: base.into().trim_end_matches('/').to_owned(),
            workspace: None,
        }
    }
    pub fn from_env() -> Self {
        Self::new(
            std::env::var("SOLMU_BACKEND_URL").unwrap_or_else(|_| "http://127.0.0.1:3000".into()),
        )
    }
    pub fn with_workspace(mut self, workspace: std::path::PathBuf) -> Self {
        self.workspace = Some(workspace.to_string_lossy().into_owned());
        self
    }
    pub async fn profile(&self) -> Result<Profile, String> {
        self.json(Method::GET, "/profile", None).await
    }
    pub async fn save_profile(
        &self,
        system_prompt: &str,
        model: Option<&str>,
    ) -> Result<Profile, String> {
        self.json(
            Method::PUT,
            "/profile",
            Some(json!({"system_prompt": system_prompt, "model": model})),
        )
        .await
    }
    pub async fn models(&self) -> Result<ModelCatalog, String> {
        self.json(Method::GET, "/models", None).await
    }
    pub async fn skills(&self, thread: &str) -> Result<SkillCatalog, String> {
        self.json(Method::GET, &format!("/threads/{thread}/skills"), None)
            .await
    }
    pub async fn mcp(&self, thread: &str) -> Result<McpCatalog, String> {
        self.json(Method::GET, &format!("/threads/{thread}/mcp"), None)
            .await
    }
    pub async fn plugins(&self, thread: &str) -> Result<PluginCatalog, String> {
        self.json(Method::GET, &format!("/threads/{thread}/plugins"), None)
            .await
    }
    pub async fn audit(&self, before: Option<i64>, limit: u32) -> Result<AuditPage, String> {
        let path = match before {
            Some(cursor) => format!("/audit?limit={limit}&before={cursor}"),
            None => format!("/audit?limit={limit}"),
        };
        self.json(Method::GET, &path, None).await
    }
    pub async fn tasks(&self) -> Result<Vec<ScheduledTask>, String> {
        #[derive(Deserialize)]
        struct Page {
            items: Vec<ScheduledTask>,
        }
        self.json::<Page>(Method::GET, "/tasks", None)
            .await
            .map(|page| page.items)
    }
    pub async fn create_task(
        &self,
        name: &str,
        prompt: &str,
        kind: &str,
        schedule: &str,
    ) -> Result<ScheduledTask, String> {
        self.json(Method::POST, "/tasks", Some(json!({"name":name,"prompt":prompt,"schedule_kind":kind,"schedule":schedule,"workspace":self.workspace}))).await
    }
    pub async fn update_task(&self, id: &str, changes: Value) -> Result<ScheduledTask, String> {
        self.json(Method::PATCH, &format!("/tasks/{id}"), Some(changes))
            .await
    }
    pub async fn delete_task(&self, id: &str) -> Result<(), String> {
        self.request(Method::DELETE, &format!("/tasks/{id}"), None, false)
            .await
            .map(|_| ())
    }
    pub async fn run_task(&self, id: &str) -> Result<TaskRun, String> {
        self.json(Method::POST, &format!("/tasks/{id}/run"), None)
            .await
    }
    pub async fn task_runs(&self, id: &str) -> Result<Vec<TaskRun>, String> {
        #[derive(Deserialize)]
        struct Page {
            items: Vec<TaskRun>,
        }
        self.json::<Page>(Method::GET, &format!("/tasks/{id}/runs"), None)
            .await
            .map(|page| page.items)
    }
    pub async fn stop(&self, id: &str) -> Result<(), String> {
        self.request(Method::POST, &format!("/threads/{id}/stop"), None, false)
            .await
            .map(|_| ())
    }
    async fn request(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
        stream: bool,
    ) -> Result<Response, String> {
        self.request_with_error(method, path, body, stream)
            .await
            .map_err(|error| error.message)
    }
    async fn request_with_error(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
        stream: bool,
    ) -> Result<Response, RequestFailure> {
        let mut request = self
            .client
            .request(method, format!("{}/api/v1{path}", self.base));
        if !stream {
            request = request.timeout(Duration::from_secs(30));
        }
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request.send().await.map_err(|_| RequestFailure {
            code: None,
            message: format!(
                "Cannot reach Solmu at {}. Check that the backend is running.",
                self.base
            ),
        })?;
        if response.status().is_success() {
            return Ok(response);
        }
        let status = response.status();
        let body = response.json::<Value>().await.unwrap_or_default();
        Err(RequestFailure {
            code: body["error"]["code"].as_str().map(str::to_owned),
            message: body["error"]["message"]
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| format!("Request failed ({status})")),
        })
    }
    async fn json<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
    ) -> Result<T, String> {
        self.request(method, path, body, false)
            .await?
            .json()
            .await
            .map_err(|_| "Invalid response from Solmu".into())
    }
    async fn list<T: DeserializeOwned>(&self, path: &str) -> Result<Vec<T>, String> {
        #[derive(Deserialize)]
        struct Page<T> {
            items: Vec<T>,
        }
        let mut items = Vec::new();
        loop {
            let page: Page<T> = self
                .json(
                    Method::GET,
                    &format!("{path}?limit=100&offset={}", items.len()),
                    None,
                )
                .await?;
            let last = page.items.len() < 100;
            items.extend(page.items);
            if last {
                return Ok(items);
            }
        }
    }
    pub fn run(&self, current: Option<Thread>, action: Action) -> Updates {
        let api = self.clone();
        Box::pin(async_stream::stream! {
            match action {
                Action::List => match api.list("/threads").await { Ok(threads) => yield Update::Threads(threads), Err(error) => { yield Update::Failed(error); return; } },
                Action::Send(content) => {
                    let Some(thread) = current else { yield Update::Failed("Create or open a conversation first".into()); return; };
                    let message: Message = match api.json(Method::POST, &format!("/threads/{}/messages", thread.id), Some(json!({"content": content}))).await {
                        Ok(message) => message, Err(error) => { yield Update::Failed(error); return; }
                    };
                    let message_id = message.id.clone();
                    yield Update::Saved(message);
                    let response = match api.request_with_error(Method::POST, &format!("/threads/{}/responses", thread.id), Some(json!({"message_id": message_id})), true).await {
                        Ok(response) => response,
                        Err(error) if error.code.as_deref() == Some("response_stopped") => { yield Update::Stopped; return; },
                        Err(error) => { yield Update::Failed(error.message); return; }
                    };
                    let mut events = response.bytes_stream().eventsource();
                    let mut completed = false;
                    while let Some(event) = events.next().await {
                        let event = match event { Ok(event) => event, Err(_) => { yield Update::Failed("The reply connection was interrupted".into()); return; } };
                        let data: Value = match serde_json::from_str(&event.data) { Ok(data) => data, Err(_) => { yield Update::Failed("Invalid stream data".into()); return; } };
                        match event.event.as_str() {
                            "reset" => yield Update::Reset,
                            "tool_start" | "tool_result" => match serde_json::from_value(data) {
                                Ok(run) => yield Update::Tool(run),
                                Err(_) => { yield Update::Failed("Invalid tool activity".into()); return; }
                            },
                            "delta" => yield Update::Delta(data["text"].as_str().unwrap_or_default().into()),
                            "done" => {
                                match serde_json::from_value(data) { Ok(message) => yield Update::Saved(message), Err(_) => { yield Update::Failed("Invalid completed reply".into()); return; } }
                                completed = true;
                            },
                            "error" => { yield Update::Failed(data["error"]["message"].as_str().unwrap_or("Reply failed").into()); return; },
                            "stopped" => { yield Update::Stopped; return; },
                            _ => {},
                        }
                    }
                    if !completed { yield Update::Failed("The reply ended before completion".into()); return; }
                    match api.list("/threads").await { Ok(threads) => yield Update::Threads(threads), Err(error) => { yield Update::Failed(error); return; } }
                },
                action => {
                    let result: Result<Option<Thread>, String> = async {
                        match action {
                            Action::New(title) => api.json(Method::POST, "/threads", Some(json!({"title": title, "workspace": api.workspace}))).await.map(Some),
                            Action::Open(id) => api.json(Method::GET, &format!("/threads/{id}"), None).await.map(Some),
                            Action::Rename(title) => {
                                let thread = current.as_ref().ok_or("No conversation selected")?;
                                api.json(Method::PATCH, &format!("/threads/{}", thread.id), Some(json!({"title": title}))).await.map(Some)
                            },
                            Action::Model(model) => {
                                let thread = current.as_ref().ok_or("No conversation selected")?;
                                api.json(Method::PATCH, &format!("/threads/{}", thread.id), Some(json!({"model": model}))).await.map(Some)
                            },
                            Action::Delete => {
                                let thread = current.as_ref().ok_or("No conversation selected")?;
                                api.request(Method::DELETE, &format!("/threads/{}", thread.id), None, false).await?;
                                Ok(None)
                            },
                            Action::Refresh => {
                                let threads: Vec<Thread> = api.list("/threads").await?;
                                Ok(current.and_then(|current| threads.into_iter().find(|thread| thread.id == current.id)))
                            },
                            Action::Send(_) => unreachable!(),
                            Action::List => unreachable!(),
                        }
                    }.await;
                    let thread = match result { Ok(thread) => thread, Err(error) => { yield Update::Failed(error); return; } };
                    let messages = if let Some(thread) = &thread {
                        match api.list(&format!("/threads/{}/messages", thread.id)).await { Ok(messages) => messages, Err(error) => { yield Update::Failed(error); return; } }
                    } else { Vec::new() };
                    let tools = if let Some(thread) = &thread {
                        match api.list(&format!("/threads/{}/tools",thread.id)).await {
                            Ok(tools)=>tools,
                            Err(error)=>{yield Update::Failed(error);return;}
                        }
                    } else {Vec::new()};
                    let skills = if let Some(thread) = &thread {
                        match api.skills(&thread.id).await {
                            Ok(skills) => skills,
                            Err(error) => SkillCatalog { directory: String::new(), items: Vec::new(), issues: vec![SkillIssue { path: ".agents/skills/".into(), message: error }] },
                        }
                    } else { SkillCatalog::default() };
                    let mcp = if let Some(thread) = &thread {
                        match api.mcp(&thread.id).await {
                            Ok(mcp) => mcp,
                            Err(error) => McpCatalog { issues: vec![SkillIssue { path: "MCP".into(), message: error }], ..McpCatalog::default() },
                        }
                    } else { McpCatalog::default() };
                    let plugins = if let Some(thread) = &thread {
                        api.plugins(&thread.id).await.unwrap_or_default()
                    } else { PluginCatalog::default() };
                    yield Update::Opened(thread, messages);
                    yield Update::Tools(tools);
                    yield Update::Skills(skills);
                    yield Update::Mcp(mcp);
                    yield Update::Plugins(plugins);
                    match api.list("/threads").await { Ok(threads) => yield Update::Threads(threads), Err(error) => { yield Update::Failed(error); return; } }
                },
            }
            yield Update::Finished;
        })
    }
}

pub type Updates = Pin<Box<dyn Stream<Item = Update> + Send>>;
#[derive(Clone, Debug)]
pub enum Connection {
    Connected,
    Changed,
    ProfileChanged,
    TasksChanged,
    Disconnected,
}
#[derive(Clone, Debug)]
pub enum Action {
    New(String),
    Open(String),
    Rename(String),
    Model(Option<String>),
    Delete,
    Refresh,
    List,
    Send(String),
}
#[derive(Clone, Debug)]
pub enum Update {
    Opened(Option<Thread>, Vec<Message>),
    Threads(Vec<Thread>),
    Saved(Message),
    Delta(String),
    Failed(String),
    Finished,
    Stopped,
    Reset,
    Tool(ToolRun),
    Tools(Vec<ToolRun>),
    Skills(SkillCatalog),
    Mcp(McpCatalog),
    Plugins(PluginCatalog),
}

pub struct Session {
    pub api: Api,
    pub current: Option<Thread>,
    pub threads: Vec<Thread>,
    pub messages: Vec<Message>,
    pub tools: Vec<ToolRun>,
    pub skills: SkillCatalog,
    pub mcp: McpCatalog,
    pub plugins: PluginCatalog,
    pub partial: String,
    pub error: Option<String>,
    pub busy: bool,
    pub responding: bool,
}
impl Session {
    pub fn new(api: Api) -> Self {
        Self {
            api,
            current: None,
            threads: Vec::new(),
            messages: Vec::new(),
            tools: Vec::new(),
            skills: SkillCatalog::default(),
            mcp: McpCatalog::default(),
            plugins: PluginCatalog::default(),
            partial: String::new(),
            error: None,
            busy: false,
            responding: false,
        }
    }
    pub fn begin(&mut self, action: Action) -> Option<Updates> {
        if self.busy {
            return None;
        }
        if matches!(action, Action::New(_)) && self.current.is_some() && self.messages.is_empty() {
            return None;
        }
        self.responding = matches!(action, Action::Send(_));
        self.busy = true;
        self.error = None;
        self.partial.clear();
        Some(self.api.run(self.current.clone(), action))
    }
    pub fn refresh(&mut self) -> Option<Updates> {
        let error = self.error.clone();
        let updates = self.begin(Action::Refresh);
        self.error = error;
        updates
    }
    pub fn stopping(&mut self) {
        self.responding = false;
        self.partial.clear();
        self.error = None;
        self.busy = true;
    }
    pub fn apply(&mut self, update: Update) {
        match update {
            Update::Reset => self.partial.clear(),
            Update::Tools(tools) => self.tools = tools,
            Update::Skills(skills) => self.skills = skills,
            Update::Mcp(mcp) => self.mcp = mcp,
            Update::Plugins(plugins) => self.plugins = plugins,
            Update::Tool(run) => {
                if let Some(existing) = self.tools.iter_mut().find(|tool| tool.id == run.id) {
                    *existing = run;
                } else {
                    self.tools.push(run);
                }
            }
            Update::Opened(thread, messages) => {
                self.current = thread;
                self.messages = messages;
            }
            Update::Threads(threads) => {
                if let Some(current) = &mut self.current
                    && let Some(thread) = threads.iter().find(|thread| thread.id == current.id)
                {
                    *current = thread.clone();
                }
                self.threads = threads;
            }
            Update::Saved(message) => {
                if message.role == "assistant" {
                    self.partial.clear();
                }
                self.messages.push(message);
            }
            Update::Delta(text) => self.partial.push_str(&text),
            Update::Failed(error) => {
                self.error = Some(error);
                self.busy = false;
                self.responding = false;
                self.partial.clear();
            }
            Update::Finished => {
                self.busy = false;
                self.responding = false;
            }
            Update::Stopped => {
                self.busy = false;
                self.responding = false;
                self.partial.clear();
            }
        }
    }
}
