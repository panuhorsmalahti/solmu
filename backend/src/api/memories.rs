use super::{
    error::{ApiError, ApiJson},
    state::AppState,
};
use crate::storage::memories;
use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct PageQuery {
    limit: Option<u32>,
    offset: Option<u32>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryInput {
    content: String,
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
pub async fn create(
    State(state): State<AppState>,
    ApiJson(input): ApiJson<MemoryInput>,
) -> Result<(StatusCode, Json<memories::Memory>), ApiError> {
    validate(&input.content)?;
    let memory = memories::write(&state.pool, &input.content).await?;
    state.memories_changed();
    Ok((StatusCode::CREATED, Json(memory)))
}
pub async fn update(
    State(state): State<AppState>,
    Path(id): Path<String>,
    ApiJson(input): ApiJson<MemoryInput>,
) -> Result<Json<memories::Memory>, ApiError> {
    validate(&input.content)?;
    let memory = memories::update(&state.pool, &id, &input.content).await?;
    state.memories_changed();
    Ok(Json(memory))
}
pub async fn delete(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    memories::delete(&state.pool, &id).await?;
    state.memories_changed();
    Ok(StatusCode::NO_CONTENT)
}
fn validate(content: &str) -> Result<(), ApiError> {
    if content.trim().is_empty() || content.len() > 20_000 {
        Err(ApiError::invalid(
            "content must contain 1 to 20000 characters",
        ))
    } else {
        Ok(())
    }
}
