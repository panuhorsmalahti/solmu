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
use crate::storage::messages::{self, Message};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateMessage {
    content: String,
}

pub async fn create(
    State(state): State<AppState>,
    Path(id): Path<String>,
    ApiJson(input): ApiJson<CreateMessage>,
) -> Result<(StatusCode, Json<Message>), ApiError> {
    let _guard = state.lock_thread(&id)?;
    if input.content.trim().is_empty() {
        return Err(ApiError::invalid("content must not be empty"));
    }
    let message = messages::create(&state.pool, &id, "user", &input.content, None).await?;
    let (count,): (i64,) = sqlx::query_as("SELECT count(*) FROM messages WHERE thread_id = ? AND role = 'user'")
        .bind(&id).fetch_one(&state.pool).await.map_err(crate::storage::StoreError::from)?;
    if count == 1 {
        let content = input.content;
        tokio::spawn(async move {
            if let Some(title) = state.llm.title(&content).await {
                // A concurrent rename or deletion takes precedence over generated titles.
                let _ = sqlx::query("UPDATE threads SET title = ? WHERE id = ? AND title = 'New conversation'")
                    .bind(title).bind(id).execute(&state.pool).await;
            }
        });
    }
    Ok((StatusCode::CREATED, Json(message)))
}

pub async fn list(
    State(state): State<AppState>,
    Path(id): Path<String>,
    query: Result<Query<Pagination>, QueryRejection>,
) -> Result<Json<ListResponse<Message>>, ApiError> {
    let (limit, offset) = query?.0.values()?;
    Ok(Json(ListResponse {
        items: messages::list(&state.pool, &id, limit, offset).await?,
        limit,
        offset,
    }))
}
