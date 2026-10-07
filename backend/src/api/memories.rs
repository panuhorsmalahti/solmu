use super::{error::ApiError, state::AppState};
use crate::storage::memories;
use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct PageQuery {
    limit: Option<u32>,
    offset: Option<u32>,
}
pub async fn list(
    State(state): State<AppState>,
    Query(query): Query<PageQuery>,
) -> Result<Json<memories::Page>, ApiError> {
    let limit = query.limit.unwrap_or(50);
    let offset = query.offset.unwrap_or(0);
    if !(1..=100).contains(&limit) {
        return Err(ApiError::invalid("limit must be between 1 and 100"));
    }
    Ok(Json(memories::page(&state.pool, limit, offset).await?))
}
pub async fn get(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<memories::Memory>, ApiError> {
    Ok(Json(memories::get(&state.pool, &id).await?))
}
