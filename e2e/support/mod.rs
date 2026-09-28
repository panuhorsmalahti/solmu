use std::{
    convert::Infallible,
    io::{BufRead, BufReader},
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
    time::Duration,
};

pub mod mcp;
pub mod plugins;
mod screenshot;
pub mod skills;
mod tools;
pub use screenshot::capture_terminal;

use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::{
        IntoResponse, Response,
        sse::{Event, Sse},
    },
    routing::post,
};
use serde_json::{Value, json};
use tempfile::TempDir;
use tokio::{net::TcpListener, task::JoinHandle};

pub const CREDENTIAL_KEYS: &[&str] = &[
    "OPENAI_API_KEY",
    "ANTHROPIC_API_KEY",
    "GEMINI_API_KEY",
    "XAI_API_KEY",
    "GROQ_API_KEY",
    "DEEPSEEK_API_KEY",
    "OPEN_ROUTER_API_KEY",
    "TOGETHER_API_KEY",
    "FIREWORKS_API_KEY",
    "COHERE_API_KEY",
    "NEBIUS_API_KEY",
    "MIMO_API_KEY",
    "MOONSHOT_API_KEY",
    "MINIMAX_API_KEY",
    "ZAI_API_KEY",
    "BIGMODEL_API_KEY",
    "ALIYUN_API_KEY",
    "BAIDU_API_KEY",
    "AIHUBMIX_API_KEY",
    "GITHUB_TOKEN",
    "OPENCODE_GO_API_KEY",
    "OLLAMA_API_KEY",
    "VERTEX_API_KEY",
    "BEDROCK_API_KEY",
];

pub fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_owned()
}

pub fn binary(name: &str) -> PathBuf {
    let key = format!("{}_BIN", name.replace('-', "_").to_uppercase());
    if let Some(path) = std::env::var_os(key) {
        return path.into();
    }
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| root().join("target"));
    target
        .join("debug")
        .join(format!("{name}{}", std::env::consts::EXE_SUFFIX))
}

pub type Requests = Arc<Mutex<Vec<(String, Value)>>>;
#[derive(Default)]
struct ReplyControl {
    remaining: std::sync::atomic::AtomicUsize,
    before_start: std::sync::atomic::AtomicUsize,
    release: tokio::sync::Notify,
}
type ProviderState = (Requests, Arc<ReplyControl>);
static STARTUP_LIMIT: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(2);

pub struct Backend {
    pub url: String,
    pub client: reqwest::Client,
    pub directory: TempDir,
    pub requests: Requests,
    child: Option<Child>,
    provider: JoinHandle<()>,
    endpoint: String,
    explicit_provider: Option<String>,
    credentials: bool,
    dotenv: bool,
    title_model: Option<String>,
    reply_control: Arc<ReplyControl>,
    boxed: bool,
    routed: bool,
}

impl Backend {
    pub async fn start() -> Self {
        Self::configured(None, true, false).await
    }

    pub async fn configured(provider: Option<&str>, credentials: bool, dotenv: bool) -> Self {
        Self::configured_with_boxer(provider, credentials, dotenv, false, false).await
    }

    pub async fn boxed() -> Self {
        Self::configured_with_boxer(None, true, false, true, false).await
    }

    #[cfg(target_os = "linux")]
    pub async fn routed() -> Self {
        Self::configured_with_boxer(None, true, false, true, true).await
    }

    async fn configured_with_boxer(
        provider: Option<&str>,
        credentials: bool,
        dotenv: bool,
        boxed: bool,
        routed: bool,
    ) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}/v1/", listener.local_addr().unwrap());
        let requests = Requests::default();
        let reply_control = Arc::new(ReplyControl::default());
        let router = Router::new()
            .route("/v1/chat/completions", post(openai))
            .route("/v1/messages", post(anthropic))
            .with_state((requests.clone(), reply_control.clone()));
        let provider_task = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        let mut backend = Self {
            url: String::new(),
            client: reqwest::Client::new(),
            directory: tempfile::tempdir().unwrap(),
            requests,
            child: None,
            provider: provider_task,
            endpoint,
            explicit_provider: provider.map(str::to_owned),
            credentials,
            dotenv,
            title_model: None,
            reply_control,
            boxed,
            routed,
        };
        if dotenv {
            std::fs::write(backend.directory.path().join(".env"), format!("LLM_PROVIDER=openai\nLLM_MODEL=test-model\nLLM_ENDPOINT={}\nOPENAI_API_KEY=fixture-key\nANTHROPIC_API_KEY=fixture-key\nSOLMU_BIND_ADDR=invalid-dotenv-value\n", backend.endpoint)).unwrap();
        } else {
            // Stop dotenv's parent-directory search before it reaches real keys.
            std::fs::write(backend.directory.path().join(".env"), "").unwrap();
        }
        backend.launch().await;
        backend
    }

    pub fn hold_next_reply(&self) {
        self.reply_control
            .remaining
            .store(1, std::sync::atomic::Ordering::SeqCst);
    }
    pub fn release_reply(&self) {
        self.reply_control.release.notify_one();
    }

    pub fn hold_next_reply_start(&self) {
        self.reply_control
            .before_start
            .store(1, std::sync::atomic::Ordering::SeqCst);
    }

    async fn launch(&mut self) {
        // Bound simultaneous native launches (not test execution), avoiding
        // startup resource contention on Windows runners.
        let _startup = STARTUP_LIMIT.acquire().await.unwrap();
        let bind = if self.routed {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            listener.local_addr().unwrap().to_string()
        } else {
            std::env::var("SOLMU_FIXTURE_BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:0".into())
        };
        let mut command = if self.boxed {
            let mut command = Command::new(binary("boxer"));
            command
                .args(["--profile", "solmu", "--cwd"])
                .arg(self.directory.path());
            if self.routed {
                let provider = self
                    .endpoint
                    .strip_prefix("http://")
                    .unwrap()
                    .strip_suffix("/v1/")
                    .unwrap();
                command.args([
                    "--isolated",
                    "--network",
                    "proxy",
                    "--allow-local",
                    provider,
                    "--publish",
                    bind.rsplit_once(':').unwrap().1,
                ]);
            }
            command.arg("--").arg(binary("solmu-backend"));
            command
        } else {
            Command::new(binary("solmu-backend"))
        };
        command
            .current_dir(self.directory.path())
            .env("SOLMU_BIND_ADDR", bind)
            .env("SOLMU_WEB_DIR", self.directory.path().join("web"))
            .env("SOLMU_DATABASE_URL", "sqlite://solmu.db")
            .env("SOLMU_WORKSPACE", self.directory.path().join("workspace"))
            .env_remove("LLM_PROVIDER")
            .env_remove("LLM_MODEL")
            .env_remove("LLM_TITLE_MODEL")
            .env_remove("LLM_ENDPOINT")
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());
        for key in CREDENTIAL_KEYS {
            command.env_remove(key);
        }
        if self.credentials && !self.dotenv {
            command
                .env("OPENAI_API_KEY", "fixture-key")
                .env("ANTHROPIC_API_KEY", "fixture-key")
                .env("LLM_MODEL", "test-model")
                .env("LLM_ENDPOINT", &self.endpoint);
        }
        if let Some(provider) = &self.explicit_provider {
            command.env("LLM_PROVIDER", provider);
        }
        if let Some(model) = &self.title_model {
            command.env("LLM_TITLE_MODEL", model);
        }
        self.child = Some(
            command
                .spawn()
                .expect("Build the backend with cargo build --workspace before running e2e tests"),
        );
        let stdout = self.child.as_mut().unwrap().stdout.take().unwrap();
        let (send, receive) = tokio::sync::oneshot::channel();
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            let _ = send.send(line);
            for line in reader.lines() {
                if line.is_err() {
                    break;
                }
            }
        });
        let line = tokio::time::timeout(Duration::from_secs(30), receive)
            .await
            .expect("Backend did not start within 30 seconds")
            .unwrap();
        self.url = line
            .trim()
            .strip_prefix("Solmu listening on ")
            .expect("Backend failed to report its address")
            .to_owned();
    }

    pub async fn restart(&mut self) {
        self.stop();
        self.launch().await;
    }

    pub async fn set_title_model(&mut self, model: &str) {
        self.title_model = Some(model.to_owned());
        self.restart().await;
    }

    pub fn stop(&mut self) {
        if let Some(mut child) = self.child.take() {
            #[cfg(unix)]
            if self.routed {
                unsafe {
                    libc::kill(child.id() as i32, libc::SIGTERM);
                }
            } else {
                let _ = child.kill();
            }
            #[cfg(not(unix))]
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    pub fn endpoint(&self, path: &str) -> String {
        format!("{}{path}", self.url)
    }

    pub async fn create_thread(&self, title: &str) -> Value {
        let response = self
            .client
            .post(self.endpoint("/api/v1/threads"))
            .json(&json!({"title": title}))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
        response.json().await.unwrap()
    }

    pub async fn send_message(&self, id: &str, content: &str) -> Value {
        let response = self
            .client
            .post(self.endpoint(&format!("/api/v1/threads/{id}/messages")))
            .json(&json!({"content": content}))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
        response.json().await.unwrap()
    }

    pub async fn messages(&self, id: &str) -> Value {
        self.client
            .get(self.endpoint(&format!("/api/v1/threads/{id}/messages")))
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap()
            .json()
            .await
            .unwrap()
    }

    pub async fn threads(&self) -> Value {
        self.client
            .get(self.endpoint("/api/v1/threads"))
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap()
            .json()
            .await
            .unwrap()
    }
}

impl Drop for Backend {
    fn drop(&mut self) {
        self.stop();
        self.provider.abort();
    }
}

async fn openai(
    State((requests, control)): State<ProviderState>,
    Json(body): Json<Value>,
) -> Response {
    provider_response("openai", requests, control, body)
}
async fn anthropic(
    State((requests, control)): State<ProviderState>,
    Json(body): Json<Value>,
) -> Response {
    provider_response("anthropic", requests, control, body)
}

fn provider_response(
    kind: &'static str,
    requests: Requests,
    control: Arc<ReplyControl>,
    body: Value,
) -> Response {
    let latest_user = body["messages"]
        .as_array()
        .and_then(|messages| {
            messages
                .iter()
                .rev()
                .find(|message| message["role"] == "user")
        })
        .map(|message| message["content"].to_string())
        .unwrap_or_default();
    let fail = latest_user.contains("FAIL");
    let truncate = latest_user.contains("TRUNCATE");
    let non_stream = body["stream"] != true;
    let model = body["model"].as_str().unwrap_or_default().to_owned();
    let calls = if non_stream {
        None
    } else {
        tools::calls(&body)
    };
    requests.lock().unwrap().push((kind.into(), body));
    if model == "unknown-title-model" {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({"error":{"message":"Unknown model"}})),
        )
            .into_response();
    }
    if fail {
        return (
            StatusCode::BAD_GATEWAY,
            Json(json!({"error": {"message": "fixture failure"}})),
        )
            .into_response();
    }
    if non_stream {
        let title = "A new idea";
        return Json(if kind == "anthropic" {
            json!({"id":"title", "type":"message", "role":"assistant", "model":model, "content":[{"type":"text", "text":title}], "stop_reason":"end_turn", "usage":{"input_tokens":1,"output_tokens":3}})
        } else {
            json!({"id":"title", "object":"chat.completion", "created":0, "model":model, "choices":[{"index":0,"message":{"role":"assistant", "content":title},"finish_reason":"stop"}],"usage":{"prompt_tokens":1,"completion_tokens":3,"total_tokens":4}})
        }).into_response();
    }
    if let Some(calls) = calls {
        return tools::response(kind, calls);
    }
    let held = control
        .remaining
        .fetch_update(
            std::sync::atomic::Ordering::SeqCst,
            std::sync::atomic::Ordering::SeqCst,
            |remaining| remaining.checked_sub(1),
        )
        .is_ok();
    let held_start = control
        .before_start
        .fetch_update(
            std::sync::atomic::Ordering::SeqCst,
            std::sync::atomic::Ordering::SeqCst,
            |remaining| remaining.checked_sub(1),
        )
        .is_ok();
    let stream = async_stream::stream! {
        if held_start { control.release.notified().await; }
        if kind == "anthropic" {
            yield Ok::<_, Infallible>(Event::default().event("message_start").data(json!({"type":"message_start","message":{"id":"fixture","type":"message","role":"assistant","content":[],"model":"test-model","usage":{"input_tokens":1,"output_tokens":0}}}).to_string()));
            yield Ok(Event::default().event("content_block_start").data(json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}).to_string()));
        }
        for (index, text) in ["Hello", " from Solmu"].into_iter().enumerate() {
            if index > 0 {
                if held { control.release.notified().await; }
                else { tokio::time::sleep(Duration::from_secs(1)).await; }
            }
            let data = if kind == "anthropic" {
                json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":text}})
            } else {
                json!({"id":"fixture","object":"chat.completion.chunk","created":0,"model":"test-model","choices":[{"index":0,"delta":{"content":text},"finish_reason":null}]})
            };
            let event = Event::default().data(data.to_string());
            yield Ok(if kind == "anthropic" { event.event("content_block_delta") } else { event });
            if truncate { return; }
        }
        if kind == "anthropic" {
            yield Ok(Event::default().event("content_block_stop").data("{\"type\":\"content_block_stop\",\"index\":0}"));
            yield Ok(Event::default().event("message_delta").data("{\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":2}}"));
            yield Ok(Event::default().event("message_stop").data("{\"type\":\"message_stop\"}"));
        } else {
            yield Ok(Event::default().data(json!({"choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}).to_string()));
            yield Ok(Event::default().data("[DONE]"));
        }
    };
    Sse::new(stream).into_response()
}
