use super::config::Config;
use rmcp::{
    ClientHandler, ClientLifecycleMode, ClientServiceExt, RoleClient,
    model::{
        ClientCapabilities, ClientConfig, Implementation, ProtocolVersion, SubscriptionFilter,
    },
    service::{NotificationContext, RunningService},
    transport::{
        StreamableHttpClientTransport, TokioChildProcess,
        streamable_http_client::StreamableHttpClientTransportConfig,
    },
};
use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

pub struct Handler {
    pub dirty: Arc<AtomicBool>,
}
impl ClientHandler for Handler {
    fn get_info(&self) -> ClientConfig {
        ClientConfig::new(
            ClientCapabilities::default(),
            Implementation::new("solmu", env!("CARGO_PKG_VERSION")),
        )
    }
    async fn on_tool_list_changed(&self, _: NotificationContext<RoleClient>) {
        self.dirty.store(true, Ordering::Release);
    }
}
pub type Client = RunningService<RoleClient, Handler>;

pub async fn connect(config: &Config, workspace: &Path) -> Result<Arc<Client>, String> {
    let dirty = Arc::new(AtomicBool::new(true));
    let handler = Handler {
        dirty: dirty.clone(),
    };
    let lifecycle = ClientLifecycleMode::Auto {
        preferred_versions: vec![ProtocolVersion::V_2026_07_28],
        legacy_version: Some(ProtocolVersion::V_2025_11_25),
    };
    let client = if let Some(program) = &config.command {
        let mut command = tokio::process::Command::new(program);
        command
            .args(&config.args)
            .envs(&config.env)
            .current_dir(workspace)
            .kill_on_drop(true);
        #[cfg(windows)]
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW
        let transport = TokioChildProcess::builder(command)
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|_| "Cannot start MCP command; check its path and backend permissions")?
            .0;
        handler.serve_with_lifecycle(transport, lifecycle).await
    } else {
        let mut transport =
            StreamableHttpClientTransportConfig::with_uri(config.url.as_deref().unwrap());
        for (name, value) in &config.headers {
            let name = reqwest::header::HeaderName::from_bytes(name.as_bytes())
                .map_err(|_| "Invalid MCP HTTP header name")?;
            if name.as_str().starts_with("mcp-")
                || matches!(
                    name.as_str(),
                    "host" | "content-type" | "accept" | "content-length" | "connection"
                )
            {
                return Err("MCP protocol headers cannot be overridden".into());
            }
            let mut value = reqwest::header::HeaderValue::from_str(value)
                .map_err(|_| "Invalid MCP HTTP header value")?;
            value.set_sensitive(true);
            transport.custom_headers.insert(name, value);
        }
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| "Cannot create MCP HTTP client")?;
        handler
            .serve_with_lifecycle(
                StreamableHttpClientTransport::with_client(http, transport),
                lifecycle,
            )
            .await
    }
    .map_err(
        |_| "MCP connection failed; check the server, transport, credentials, and protocol version",
    )?;
    client
        .set_response_cache_config(rmcp::service::ClientCacheConfig::disabled())
        .await;
    let client = Arc::new(client);
    if client.peer_info().is_some_and(|info| {
        info.capabilities
            .tools
            .as_ref()
            .is_some_and(|tools| tools.list_changed == Some(true))
    }) {
        let peer = Arc::downgrade(&client);
        tokio::spawn(async move {
            let Some(client) = peer.upgrade() else {
                return;
            };
            let subscription = tokio::time::timeout(
                Duration::from_secs(5),
                client.listen(SubscriptionFilter::builder().tools_list_changed().build()),
            )
            .await;
            drop(client);
            if let Ok(Ok(mut subscription)) = subscription {
                while let Ok(Some(_)) = subscription.next().await {
                    dirty.store(true, Ordering::Release);
                }
            }
        });
    }
    Ok(client)
}
