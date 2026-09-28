use super::StoreError;
use chrono::{DateTime, SecondsFormat, Utc};
use cron::Schedule;
use serde::Serialize;
use sqlx::{FromRow, SqlitePool};
use std::str::FromStr;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct ScheduledTask {
    pub id: String,
    pub name: String,
    pub prompt: String,
    pub schedule_kind: String,
    pub schedule: String,
    pub thread_id: String,
    pub enabled: bool,
    pub running: bool,
    pub next_run_at: Option<String>,
    pub last_run_at: Option<String>,
    pub last_status: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct TaskRun {
    pub id: String,
    pub task_id: String,
    pub scheduled_for: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub status: String,
    pub error: Option<String>,
    pub message_id: Option<String>,
}

pub fn timestamp(time: DateTime<Utc>) -> String {
    time.to_rfc3339_opts(SecondsFormat::Millis, true)
}

pub fn next_run(kind: &str, schedule: &str, now: DateTime<Utc>) -> Result<String, String> {
    match kind {
        "once" => {
            let at = DateTime::parse_from_rfc3339(schedule)
                .map_err(|_| "One-shot time must be an RFC 3339 timestamp with a timezone")?
                .with_timezone(&Utc);
            if at <= now {
                return Err("One-shot time must be in the future".into());
            }
            Ok(timestamp(at))
        }
        "cron" => {
            let parts: Vec<_> = schedule.split_whitespace().collect();
            if parts.len() != 5 {
                return Err(
                    "Cron expression must have five fields: minute hour day month weekday".into(),
                );
            }
            let expression = format!("0 {schedule}");
            Schedule::from_str(&expression)
                .map_err(|_| "Invalid cron expression".to_owned())?
                .after(&now)
                .next()
                .map(timestamp)
                .ok_or_else(|| "Cron expression has no future occurrence".into())
        }
        _ => Err("Schedule kind must be once or cron".into()),
    }
}

pub async fn create(
    pool: &SqlitePool,
    name: &str,
    prompt: &str,
    kind: &str,
    schedule: &str,
    workspace: &str,
    next: &str,
) -> Result<ScheduledTask, StoreError> {
    let id = Uuid::new_v4().to_string();
    let thread_id = Uuid::new_v4().to_string();
    let mut tx = pool.begin().await?;
    sqlx::query("INSERT INTO threads (id,title,workspace) VALUES (?,?,?)")
        .bind(&thread_id)
        .bind(name)
        .bind(workspace)
        .execute(&mut *tx)
        .await?;
    let task = sqlx::query_as::<_, ScheduledTask>("INSERT INTO scheduled_tasks (id,name,prompt,schedule_kind,schedule,thread_id,next_run_at) VALUES (?,?,?,?,?,?,?) RETURNING *")
        .bind(id).bind(name).bind(prompt).bind(kind).bind(schedule).bind(thread_id).bind(next)
        .fetch_one(&mut *tx).await?;
    tx.commit().await?;
    Ok(task)
}

pub async fn list(pool: &SqlitePool) -> Result<Vec<ScheduledTask>, StoreError> {
    Ok(sqlx::query_as::<_, ScheduledTask>("SELECT * FROM scheduled_tasks ORDER BY enabled DESC, next_run_at IS NULL, next_run_at, created_at DESC")
        .fetch_all(pool).await?)
}

pub async fn get(pool: &SqlitePool, id: &str) -> Result<ScheduledTask, StoreError> {
    sqlx::query_as::<_, ScheduledTask>("SELECT * FROM scheduled_tasks WHERE id=?")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or(StoreError::NotFound("Scheduled task not found"))
}

pub struct TaskChanges<'a> {
    pub name: &'a str,
    pub prompt: &'a str,
    pub kind: &'a str,
    pub schedule: &'a str,
    pub enabled: bool,
    pub next: Option<&'a str>,
}

pub async fn update(
    pool: &SqlitePool,
    id: &str,
    changes: TaskChanges<'_>,
) -> Result<ScheduledTask, StoreError> {
    let old = get(pool, id).await?;
    if old.running {
        return Err(StoreError::Conflict(
            "Wait for the task run to finish before editing it",
        ));
    }
    let mut tx = pool.begin().await?;
    let task = sqlx::query_as::<_, ScheduledTask>("UPDATE scheduled_tasks SET name=?,prompt=?,schedule_kind=?,schedule=?,enabled=?,next_run_at=?,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=? AND running=0 RETURNING *")
        .bind(changes.name).bind(changes.prompt).bind(changes.kind).bind(changes.schedule).bind(changes.enabled).bind(changes.next).bind(id)
        .fetch_optional(&mut *tx).await?
        .ok_or(StoreError::Conflict("Task is running"))?;
    sqlx::query("UPDATE threads SET title=? WHERE id=? AND title=?")
        .bind(changes.name)
        .bind(&task.thread_id)
        .bind(old.name)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(task)
}

pub async fn delete(pool: &SqlitePool, id: &str) -> Result<(), StoreError> {
    let result = sqlx::query("DELETE FROM scheduled_tasks WHERE id=? AND running=0")
        .bind(id)
        .execute(pool)
        .await?;
    if result.rows_affected() == 0 {
        let task = get(pool, id).await?;
        if task.running {
            return Err(StoreError::Conflict(
                "Wait for the task run to finish before deleting it",
            ));
        }
    }
    Ok(())
}

pub async fn runs(pool: &SqlitePool, id: &str) -> Result<Vec<TaskRun>, StoreError> {
    get(pool, id).await?;
    Ok(sqlx::query_as::<_, TaskRun>("SELECT * FROM scheduled_task_runs WHERE task_id=? ORDER BY started_at DESC, id DESC LIMIT 50")
        .bind(id).fetch_all(pool).await?)
}

pub async fn recover(pool: &SqlitePool) -> Result<(), StoreError> {
    let mut tx = pool.begin().await?;
    sqlx::query("UPDATE scheduled_task_runs SET status='interrupted',error='Backend stopped during the run',finished_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE status='running'")
        .execute(&mut *tx).await?;
    sqlx::query("UPDATE scheduled_tasks SET running=0,last_status='interrupted' WHERE running=1")
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

pub async fn due(
    pool: &SqlitePool,
    now: DateTime<Utc>,
) -> Result<Vec<(ScheduledTask, TaskRun)>, StoreError> {
    let current = timestamp(now);
    let tasks = sqlx::query_as::<_, ScheduledTask>("SELECT * FROM scheduled_tasks WHERE enabled=1 AND running=0 AND next_run_at<=? ORDER BY next_run_at LIMIT 10")
        .bind(&current).fetch_all(pool).await?;
    let mut claimed = Vec::new();
    for task in tasks {
        let next = if task.schedule_kind == "cron" {
            next_run("cron", &task.schedule, now).ok()
        } else {
            None
        };
        if let Some(run) = claim(
            pool,
            &task,
            task.next_run_at.as_deref().unwrap_or(&current),
            next.as_deref(),
            false,
        )
        .await?
        {
            claimed.push((task, run));
        }
    }
    Ok(claimed)
}

pub async fn run_now(pool: &SqlitePool, id: &str) -> Result<(ScheduledTask, TaskRun), StoreError> {
    let task = get(pool, id).await?;
    if task.running {
        return Err(StoreError::Conflict("Task is already running"));
    }
    let current = timestamp(Utc::now());
    let run = claim(pool, &task, &current, task.next_run_at.as_deref(), true)
        .await?
        .ok_or(StoreError::Conflict("Task is already running"))?;
    Ok((task, run))
}

async fn claim(
    pool: &SqlitePool,
    task: &ScheduledTask,
    scheduled_for: &str,
    next: Option<&str>,
    manual: bool,
) -> Result<Option<TaskRun>, StoreError> {
    let mut tx = pool.begin().await?;
    let updated = if manual {
        sqlx::query("UPDATE scheduled_tasks SET running=1,last_run_at=?,last_status='running' WHERE id=? AND running=0")
            .bind(scheduled_for).bind(&task.id).execute(&mut *tx).await?
    } else {
        sqlx::query("UPDATE scheduled_tasks SET running=1,last_run_at=?,last_status='running',next_run_at=?,enabled=? WHERE id=? AND running=0 AND enabled=1 AND next_run_at=?")
            .bind(scheduled_for).bind(next).bind(task.schedule_kind == "cron" && next.is_some())
            .bind(&task.id).bind(scheduled_for).execute(&mut *tx).await?
    };
    if updated.rows_affected() == 0 {
        return Ok(None);
    }
    let run = sqlx::query_as::<_, TaskRun>("INSERT INTO scheduled_task_runs (id,task_id,scheduled_for,status) VALUES (?,?,?,'running') RETURNING *")
        .bind(Uuid::new_v4().to_string()).bind(&task.id).bind(scheduled_for)
        .fetch_one(&mut *tx).await?;
    tx.commit().await?;
    Ok(Some(run))
}

pub async fn finish(
    pool: &SqlitePool,
    run_id: &str,
    task_id: &str,
    status: &str,
    error: Option<&str>,
    message_id: Option<&str>,
) -> Result<(), StoreError> {
    let mut tx = pool.begin().await?;
    sqlx::query("UPDATE scheduled_task_runs SET status=?,error=?,message_id=?,finished_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?")
        .bind(status).bind(error).bind(message_id).bind(run_id).execute(&mut *tx).await?;
    sqlx::query("UPDATE scheduled_tasks SET running=0,last_status=?,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?")
        .bind(status).bind(task_id).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}
