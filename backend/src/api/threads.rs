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
    workspace: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateThread {
    title: Option<String>,
    #[serde(default, deserialize_with = "present_model")]
    model: Option<Option<String>>,
}

pub fn present_model<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Option<String>>, D::Error> {
    Ok(Some(Option::<String>::deserialize(deserializer)?))
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
    let workspace = crate::workspace::resolve(input.workspace.as_deref())?;
    let thread = threads::create(&state.pool, title, &workspace).await?;
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
    if input.title.is_none() && input.model.is_none() {
        return Err(ApiError::invalid("Provide a title or model"));
    }
    let title = input.title.as_deref().map(validate_title).transpose()?;
    if let Some(Some(model)) = &input.model {
        state.llm.validate_model(model)?;
    }
    let model = input.model.as_ref().map(|model| model.as_deref());
    let thread = threads::update(&state.pool, &id, title, model).await?;
    state.changed(&id);
    Ok(Json(thread))
}

pub async fn delete(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let _guard = state.lock_thread(&id)?;
    threads::delete(&state.pool, &id).await?;
    state.skills.forget(&id).await;
    state.mcp.forget(&id).await;
    state.changed(&id);
    Ok(StatusCode::NO_CONTENT)
}
