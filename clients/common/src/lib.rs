use eventsource_stream::Eventsource;
use futures_util::{Stream, StreamExt};
use reqwest::{Client, Method, Response};
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::{pin::Pin, time::Duration};

#[derive(Debug, Clone, Deserialize)]
pub struct Thread {
    pub id: String,
    pub title: String,
}
#[derive(Debug, Clone, Deserialize)]
pub struct Message {
    pub id: String,
    pub role: String,
    pub content: String,
}

#[derive(Clone)]
pub struct Api {
    client: Client,
    base: String,
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
                    while let Some(Ok(message)) = socket.next().await {
                        if message.is_close() { break; }
                        if let Ok(text) = message.to_text()
                            && serde_json::from_str::<Value>(text).ok().is_some_and(|event| event["type"] == "conversation_changed") {
                            yield Connection::Changed;
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
        }
    }
    pub fn from_env() -> Self {
        Self::new(
            std::env::var("SOLMU_BACKEND_URL").unwrap_or_else(|_| "http://127.0.0.1:3000".into()),
        )
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
        let mut request = self
            .client
            .request(method, format!("{}/api/v1{path}", self.base));
        if !stream {
            request = request.timeout(Duration::from_secs(30));
        }
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request.send().await.map_err(|_| {
            format!(
                "Cannot reach Solmu at {}. Check that the backend is running.",
                self.base
            )
        })?;
        if response.status().is_success() {
            return Ok(response);
        }
        let status = response.status();
        let body = response.json::<Value>().await.unwrap_or_default();
        Err(body["error"]["message"]
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| format!("Request failed ({status})")))
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
                    let response = match api.request(Method::POST, &format!("/threads/{}/responses", thread.id), Some(json!({"message_id": message_id})), true).await {
                        Ok(response) => response, Err(error) => { yield Update::Failed(error); return; }
                    };
                    let mut events = response.bytes_stream().eventsource();
                    let mut completed = false;
                    while let Some(event) = events.next().await {
                        let event = match event { Ok(event) => event, Err(_) => { yield Update::Failed("The reply connection was interrupted".into()); return; } };
                        let data: Value = match serde_json::from_str(&event.data) { Ok(data) => data, Err(_) => { yield Update::Failed("Invalid stream data".into()); return; } };
                        match event.event.as_str() {
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
                            Action::New(title) => api.json(Method::POST, "/threads", Some(json!({"title": title}))).await.map(Some),
                            Action::Open(id) => api.json(Method::GET, &format!("/threads/{id}"), None).await.map(Some),
                            Action::Rename(title) => {
                                let thread = current.as_ref().ok_or("No conversation selected")?;
                                api.json(Method::PATCH, &format!("/threads/{}", thread.id), Some(json!({"title": title}))).await.map(Some)
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
                    yield Update::Opened(thread, messages);
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
    Disconnected,
}
#[derive(Clone, Debug)]
pub enum Action {
    New(String),
    Open(String),
    Rename(String),
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
}

pub struct Session {
    pub api: Api,
    pub current: Option<Thread>,
    pub threads: Vec<Thread>,
    pub messages: Vec<Message>,
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
