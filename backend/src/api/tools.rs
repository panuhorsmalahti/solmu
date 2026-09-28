use super::{ListResponse, Pagination, error::ApiError, state::AppState};
use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct AuditQuery {
    limit: Option<u32>,
    before: Option<i64>,
}

pub async fn audit(
    State(state): State<AppState>,
    query: Result<Query<AuditQuery>, axum::extract::rejection::QueryRejection>,
) -> Result<Json<crate::storage::tools::AuditPage>, ApiError> {
    let query = query?.0;
    let limit = query.limit.unwrap_or(25);
    if !(1..=100).contains(&limit) {
        return Err(ApiError::invalid("limit must be between 1 and 100"));
    }
    if query.before.is_some_and(|before| before <= 0) {
        return Err(ApiError::invalid(
            "before must be a positive sequence number",
        ));
    }
    Ok(Json(
        crate::storage::tools::audit(&state.pool, limit, query.before).await?,
    ))
}

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
