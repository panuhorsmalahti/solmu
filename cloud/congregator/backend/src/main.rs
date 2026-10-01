use std::{env, error::Error, net::SocketAddr, sync::Arc};

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use openshell_sdk::{
    AuthConfig, ClientConfig, DeleteOptions, ListOptions, OpenShellClient, SandboxPhase,
    SandboxSpec, ServiceExposure,
};
use serde::{Deserialize, Serialize};
use tower_http::services::ServeDir;

#[derive(Clone)]
struct AppState {
    openshell: Arc<OpenShellClient>,
    image: String,
    provider: Option<String>,
}

#[derive(Debug, Serialize)]
struct Agent {
    /// OpenShell sandbox name used to target lifecycle operations.
    id: String,
    name: String,
    status: String,
    image: String,
    created_at: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateAgent {
    name: String,
}

#[derive(Debug, Serialize)]
struct ErrorBody {
    error: String,
}

#[derive(Debug)]
struct ApiError(StatusCode, String);

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(ErrorBody { error: self.1 })).into_response()
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let _ = dotenvy::dotenv();
    tracing_subscriber::fmt()
        .with_env_filter(env::var("RUST_LOG").unwrap_or_else(|_| "info".into()))
        .init();

    let gateway = env::var("OPENSHELL_GATEWAY_URL")
        .unwrap_or_else(|_| "https://openshell.openshell.svc.cluster.local:8080".into());
    let ca_cert = env::var("OPENSHELL_CA_CERT_PATH")
        .ok()
        .map(std::fs::read)
        .transpose()?;
    let mut config = ClientConfig::default();
    config.gateway = gateway;
    config.ca_cert = ca_cert;
    if let Ok(token) = env::var("OPENSHELL_TOKEN") {
        config.auth = Some(AuthConfig::oidc(token));
    }
    let openshell = OpenShellClient::connect(config).await?;
    let health = openshell.health().await?;
    tracing::info!(version = %health.version, status = ?health.status, "Connected to OpenShell gateway");

    let state = AppState {
        openshell: Arc::new(openshell),
        image: env::var("CONGREGATOR_SOLMU_IMAGE")
            .unwrap_or_else(|_| "ghcr.io/panuhorsmalahti/solmu:latest".into()),
        provider: env::var("CONGREGATOR_OPEN_SHELL_PROVIDER")
            .ok()
            .filter(|provider| !provider.trim().is_empty()),
    };
    let web_dir = env::var("CONGREGATOR_WEB_DIR").unwrap_or_else(|_| "web".into());
    let app = api(state).nest_service("/", ServeDir::new(web_dir));
    let addr: SocketAddr = env::var("CONGREGATOR_BIND_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:8080".into())
        .parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(%addr, "Congregator is listening");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

fn api(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(|| async { StatusCode::NO_CONTENT }))
        .route("/api/v1/agents", get(list_agents).post(create_agent))
        .route("/api/v1/agents/{name}", get(get_agent).delete(delete_agent))
        .route("/api/v1/agents/{name}/stop", post(stop_agent))
        .route("/api/v1/agents/{name}/start", post(start_agent))
        .with_state(state)
}

async fn list_agents(State(state): State<AppState>) -> Result<Json<Vec<Agent>>, ApiError> {
    let sandboxes = state
        .openshell
        .list_all_sandboxes(ListOptions {
            label_selector: Some("solmu.dev/managed=true".into()),
            ..Default::default()
        })
        .await
        .map_err(gateway_error)?;
    let agents = sandboxes
        .into_iter()
        .map(|sandbox| Agent {
            id: sandbox.name.clone(),
            name: sandbox
                .labels
                .get("solmu.dev/name")
                .cloned()
                .unwrap_or(sandbox.name),
            status: phase_name(sandbox.phase),
            image: state.image.clone(),
            created_at: None,
        })
        .collect();
    Ok(Json(agents))
}

async fn get_agent(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<Agent>, ApiError> {
    let sandbox = state
        .openshell
        .get_sandbox(&name)
        .await
        .map_err(gateway_error)?;
    if sandbox.labels.get("solmu.dev/managed").map(String::as_str) != Some("true") {
        return Err(ApiError(StatusCode::NOT_FOUND, "Agent not found".into()));
    }
    Ok(Json(Agent {
        id: sandbox.name.clone(),
        name: sandbox
            .labels
            .get("solmu.dev/name")
            .cloned()
            .unwrap_or(sandbox.name),
        status: phase_name(sandbox.phase),
        image: state.image,
        created_at: None,
    }))
}

async fn create_agent(
    State(state): State<AppState>,
    Json(input): Json<CreateAgent>,
) -> Result<(StatusCode, Json<Agent>), ApiError> {
    let name = slug(&input.name);
    if name.is_empty() {
        return Err(ApiError(
            StatusCode::BAD_REQUEST,
            "Enter a name using letters or numbers".into(),
        ));
    }
    let image = state.image.clone();
    let sandbox_name = format!(
        "solmu-{name}-{}",
        &uuid::Uuid::new_v4().simple().to_string()[..8]
    );
    let sandbox = state
        .openshell
        .create_sandbox(SandboxSpec {
            name: Some(sandbox_name),
            image: Some(image.clone()),
            labels: [("solmu.dev/managed".into(), "true".into())]
                .into_iter()
                .chain([("solmu.dev/name".into(), name.clone())])
                .collect(),
            providers: state.provider.clone().into_iter().collect(),
            command: vec!["/usr/local/bin/solmu-backend".into()],
            service_exposures: vec![ServiceExposure {
                service: "solmu".into(),
                target_port: 3000,
            }],
            ..Default::default()
        })
        .await
        .map_err(gateway_error)?;
    Ok((
        StatusCode::CREATED,
        Json(Agent {
            id: sandbox.name.clone(),
            name,
            status: phase_name(sandbox.phase),
            image,
            created_at: None,
        }),
    ))
}

async fn delete_agent(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<StatusCode, ApiError> {
    require_managed(&state, &name).await?;
    state
        .openshell
        .delete_sandbox(&name, DeleteOptions::default())
        .await
        .map_err(gateway_error)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn stop_agent(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<Agent>, ApiError> {
    require_managed(&state, &name).await?;
    let sandbox = state
        .openshell
        .stop_sandbox(&name)
        .await
        .map_err(gateway_error)?;
    Ok(Json(Agent {
        id: sandbox.name.clone(),
        name: sandbox
            .labels
            .get("solmu.dev/name")
            .cloned()
            .unwrap_or(sandbox.name),
        status: phase_name(sandbox.phase),
        image: state.image,
        created_at: None,
    }))
}

async fn start_agent(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<Agent>, ApiError> {
    require_managed(&state, &name).await?;
    let sandbox = state
        .openshell
        .start_sandbox(&name)
        .await
        .map_err(gateway_error)?;
    Ok(Json(Agent {
        id: sandbox.name.clone(),
        name: sandbox
            .labels
            .get("solmu.dev/name")
            .cloned()
            .unwrap_or(sandbox.name),
        status: phase_name(sandbox.phase),
        image: state.image,
        created_at: None,
    }))
}

async fn require_managed(state: &AppState, name: &str) -> Result<(), ApiError> {
    let sandbox = state
        .openshell
        .get_sandbox(name)
        .await
        .map_err(gateway_error)?;
    if sandbox.labels.get("solmu.dev/managed").map(String::as_str) != Some("true") {
        return Err(ApiError(StatusCode::NOT_FOUND, "Agent not found".into()));
    }
    Ok(())
}

fn phase_name(phase: SandboxPhase) -> String {
    match phase {
        SandboxPhase::Provisioning | SandboxPhase::Starting => "Starting".into(),
        SandboxPhase::Ready => "Running".into(),
        SandboxPhase::Error => "Error".into(),
        SandboxPhase::Deleting => "Deleting".into(),
        SandboxPhase::Stopping => "Stopping".into(),
        SandboxPhase::Stopped => "Stopped".into(),
        SandboxPhase::Unspecified | SandboxPhase::Unknown => "Unknown".into(),
        _ => format!("{phase:?}"),
    }
}

fn slug(input: &str) -> String {
    let mut output = String::new();
    for character in input.chars().flat_map(char::to_lowercase) {
        if character.is_ascii_alphanumeric() {
            output.push(character);
        } else if !output.is_empty() && !output.ends_with('-') {
            output.push('-');
        }
    }
    output.trim_matches('-').chars().take(38).collect()
}

fn gateway_error(error: impl std::fmt::Display) -> ApiError {
    tracing::error!(%error, "OpenShell request failed");
    ApiError(
        StatusCode::BAD_GATEWAY,
        "OpenShell gateway request failed".into(),
    )
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}
