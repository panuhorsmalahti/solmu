use super::state::AppState;
use axum::{
    Json,
    extract::{Path, Query, State, rejection::QueryRejection},
    http::StatusCode,
};
use serde::Deserialize;

use super::{
    ListResponse, Pagination,
    error::{ApiError, ApiJson},
};
use crate::storage::threads::{self, Thread};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateThread {
    title: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateThread {
    title: String,
}

fn validate_title(title: &str) -> Result<&str, ApiError> {
    let title = title.trim();
    if title.is_empty() {
        return Err(ApiError::invalid("title must not be empty"));
    }
    Ok(title)
}

pub async fn create(
    State(state): State<AppState>,
    ApiJson(input): ApiJson<CreateThread>,
) -> Result<(StatusCode, Json<Thread>), ApiError> {
    let title = validate_title(input.title.as_deref().unwrap_or("New conversation"))?;
    let thread = threads::create(&state.pool, title).await?;
    state.changed(&thread.id);
    Ok((StatusCode::CREATED, Json(thread)))
}

pub async fn list(
    State(state): State<AppState>,
    query: Result<Query<Pagination>, QueryRejection>,
) -> Result<Json<ListResponse<Thread>>, ApiError> {
    let (limit, offset) = query?.0.values()?;
    Ok(Json(ListResponse {
        items: threads::list(&state.pool, limit, offset).await?,
        limit,
        offset,
    }))
}

pub async fn get(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Thread>, ApiError> {
    Ok(Json(threads::get(&state.pool, &id).await?))
}

pub async fn update(
    State(state): State<AppState>,
    Path(id): Path<String>,
    ApiJson(input): ApiJson<UpdateThread>,
) -> Result<Json<Thread>, ApiError> {
    let _guard = state.lock_thread(&id)?;
    let title = validate_title(&input.title)?;
    let thread = threads::update(&state.pool, &id, title).await?;
    state.changed(&id);
    Ok(Json(thread))
}

pub async fn delete(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let _guard = state.lock_thread(&id)?;
    threads::delete(&state.pool, &id).await?;
    state.changed(&id);
    Ok(StatusCode::NO_CONTENT)
}
