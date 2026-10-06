use super::{
    Pagination,
    error::{ApiError, ApiJson},
    state::AppState,
};
use crate::storage::goals;
use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateGoal {
    objective: String,
    thread_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateGoal {
    status: String,
}

pub async fn list(
    State(state): State<AppState>,
    Query(page): Query<Pagination>,
) -> Result<Json<super::ListResponse<goals::Goal>>, ApiError> {
    let (limit, offset) = page.values()?;
    let all = goals::list(&state.pool).await?;
    let items = all
        .into_iter()
        .skip(offset as usize)
        .take(limit as usize)
        .collect();
    Ok(Json(super::ListResponse {
        items,
        limit,
        offset,
    }))
}

pub async fn create(
    State(state): State<AppState>,
    ApiJson(input): ApiJson<CreateGoal>,
) -> Result<(StatusCode, Json<goals::Goal>), ApiError> {
    if input.objective.trim().is_empty() || input.objective.len() > 20_000 {
        return Err(ApiError::invalid(
            "objective must contain 1 to 20000 characters",
        ));
    }
    if let Some(thread_id) = input.thread_id.as_deref() {
        crate::storage::threads::get(&state.pool, thread_id).await?;
    }
    let goal = goals::create(&state.pool, &input.objective, input.thread_id.as_deref()).await?;
    state.goals_changed();
    Ok((StatusCode::CREATED, Json(goal)))
}

pub async fn update(
    State(state): State<AppState>,
    Path(id): Path<String>,
    ApiJson(input): ApiJson<UpdateGoal>,
) -> Result<Json<goals::Goal>, ApiError> {
    if !["active", "paused", "completed", "cancelled"].contains(&input.status.as_str()) {
        return Err(ApiError::invalid(
            "status must be active, paused, completed, or cancelled",
        ));
    }
    let goal = goals::set_status(&state.pool, &id, &input.status).await?;
    state.goals_changed();
    Ok(Json(goal))
}
