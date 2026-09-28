use super::{error::ApiError, state::AppState};
use axum::{
    Json,
    extract::{Path, State},
};

pub async fn list(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<crate::mcp::Catalog>, ApiError> {
    let thread = crate::storage::threads::get(&state.pool, &id).await?;
    let workspace = crate::workspace::resolve(thread.workspace.as_deref())?;
    Ok(Json(state.mcp.load(&id, workspace.into()).await.catalog))
}
