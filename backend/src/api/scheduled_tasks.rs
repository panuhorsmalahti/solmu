use super::{
    ListResponse,
    error::{ApiError, ApiJson},
    state::AppState,
};
use crate::{
    scheduler,
    storage::scheduled_tasks::{self, ScheduledTask, TaskRun},
};
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use chrono::Utc;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateTask {
    name: String,
    prompt: String,
    schedule_kind: String,
    schedule: String,
    workspace: Option<String>,
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateTask {
    name: Option<String>,
    prompt: Option<String>,
    schedule_kind: Option<String>,
    schedule: Option<String>,
    enabled: Option<bool>,
}

fn validate(name: &str, prompt: &str, kind: &str, schedule: &str) -> Result<String, ApiError> {
    if name.trim().is_empty() || name.len() > 120 {
        return Err(ApiError::invalid("Task name must be 1–120 characters"));
    }
    if prompt.trim().is_empty() || prompt.len() > 20_000 {
        return Err(ApiError::invalid("Task prompt must be 1–20,000 characters"));
    }
    if schedule.len() > 120 {
        return Err(ApiError::invalid("Schedule is too long"));
    }
    scheduled_tasks::next_run(kind, schedule, Utc::now()).map_err(ApiError::invalid)
}

pub async fn create(
    State(state): State<AppState>,
    ApiJson(input): ApiJson<CreateTask>,
) -> Result<(StatusCode, Json<ScheduledTask>), ApiError> {
    let next = validate(
        &input.name,
        &input.prompt,
        &input.schedule_kind,
        &input.schedule,
    )?;
    let workspace = crate::workspace::resolve(input.workspace.as_deref())?;
    let task = scheduled_tasks::create(
        &state.pool,
        input.name.trim(),
        input.prompt.trim(),
        &input.schedule_kind,
        input.schedule.trim(),
        &workspace,
        &next,
    )
    .await?;
    state.tasks_changed();
    state.changed(&task.thread_id);
    Ok((StatusCode::CREATED, Json(task)))
}

pub async fn list(
    State(state): State<AppState>,
) -> Result<Json<ListResponse<ScheduledTask>>, ApiError> {
    let items = scheduled_tasks::list(&state.pool).await?;
    let limit = items.len() as u32;
    Ok(Json(ListResponse {
        items,
        limit,
        offset: 0,
    }))
}

pub async fn get(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ScheduledTask>, ApiError> {
    Ok(Json(scheduled_tasks::get(&state.pool, &id).await?))
}

pub async fn update(
    State(state): State<AppState>,
    Path(id): Path<String>,
    ApiJson(input): ApiJson<UpdateTask>,
) -> Result<Json<ScheduledTask>, ApiError> {
    let old = scheduled_tasks::get(&state.pool, &id).await?;
    let name = input.name.as_deref().unwrap_or(&old.name);
    let prompt = input.prompt.as_deref().unwrap_or(&old.prompt);
    let kind = input.schedule_kind.as_deref().unwrap_or(&old.schedule_kind);
    let schedule = input.schedule.as_deref().unwrap_or(&old.schedule);
    let enabled = input.enabled.unwrap_or(old.enabled);
    let next = if enabled {
        Some(validate(name, prompt, kind, schedule)?)
    } else {
        // Validate the payload even while paused; a paused task can be edited safely.
        if name.trim().is_empty()
            || name.len() > 120
            || prompt.trim().is_empty()
            || prompt.len() > 20_000
            || schedule.len() > 120
        {
            return Err(ApiError::invalid("Invalid task name, prompt, or schedule"));
        }
        if kind != "once" && kind != "cron" {
            return Err(ApiError::invalid("Schedule kind must be once or cron"));
        }
        if kind == "cron" {
            scheduled_tasks::next_run(kind, schedule, Utc::now()).map_err(ApiError::invalid)?;
        } else {
            chrono::DateTime::parse_from_rfc3339(schedule).map_err(|_| {
                ApiError::invalid("One-shot time must be an RFC 3339 timestamp with a timezone")
            })?;
        }
        old.next_run_at.clone()
    };
    let task = scheduled_tasks::update(
        &state.pool,
        &id,
        scheduled_tasks::TaskChanges {
            name: name.trim(),
            prompt: prompt.trim(),
            kind,
            schedule: schedule.trim(),
            enabled,
            next: next.as_deref(),
        },
    )
    .await?;
    state.tasks_changed();
    state.changed(&task.thread_id);
    Ok(Json(task))
}

pub async fn delete(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    scheduled_tasks::delete(&state.pool, &id).await?;
    state.tasks_changed();
    Ok(StatusCode::NO_CONTENT)
}

pub async fn runs(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ListResponse<TaskRun>>, ApiError> {
    let items = scheduled_tasks::runs(&state.pool, &id).await?;
    let limit = items.len() as u32;
    Ok(Json(ListResponse {
        items,
        limit,
        offset: 0,
    }))
}

pub async fn run_now(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<(StatusCode, Json<TaskRun>), ApiError> {
    let (task, run) = scheduled_tasks::run_now(&state.pool, &id).await?;
    state.tasks_changed();
    tokio::spawn(scheduler::execute(state, task, run.clone()));
    Ok((StatusCode::ACCEPTED, Json(run)))
}
