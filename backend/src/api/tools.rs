use super::{ListResponse, Pagination, error::ApiError, state::AppState};
use axum::{
    Json,
    extract::{Path, Query, State},
};

pub async fn definitions(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(serde_json::json!({"items":state.tools.definitions()}))
}
pub async fn list(
    State(state): State<AppState>,
    Path(id): Path<String>,
    query: Result<Query<Pagination>, axum::extract::rejection::QueryRejection>,
) -> Result<Json<ListResponse<crate::storage::tools::ToolRun>>, ApiError> {
    let (limit, offset) = query?.0.values()?;
    Ok(Json(ListResponse {
        items: crate::storage::tools::list(&state.pool, &id, limit, offset).await?,
        limit,
        offset,
    }))
}
