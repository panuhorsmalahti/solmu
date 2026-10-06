use serde::Serialize;
use sqlx::SqlitePool;
use uuid::Uuid;

use super::StoreError;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Goal {
    pub id: String,
    pub objective: String,
    pub status: String,
    pub thread_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

pub async fn create(
    pool: &SqlitePool,
    objective: &str,
    thread_id: Option<&str>,
) -> Result<Goal, StoreError> {
    Ok(sqlx::query_as::<_, Goal>(
        "INSERT INTO goals (id, objective, thread_id) VALUES (?, ?, ?) RETURNING *",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(objective.trim())
    .bind(thread_id)
    .fetch_one(pool)
    .await?)
}

pub async fn list(pool: &SqlitePool) -> Result<Vec<Goal>, StoreError> {
    Ok(sqlx::query_as::<_, Goal>("SELECT * FROM goals ORDER BY CASE status WHEN 'active' THEN 0 WHEN 'paused' THEN 1 ELSE 2 END, created_at DESC")
        .fetch_all(pool).await?)
}

pub async fn active(pool: &SqlitePool) -> Result<Vec<Goal>, StoreError> {
    Ok(
        sqlx::query_as::<_, Goal>("SELECT * FROM goals WHERE status='active' ORDER BY created_at")
            .fetch_all(pool)
            .await?,
    )
}

pub async fn set_status(pool: &SqlitePool, id: &str, status: &str) -> Result<Goal, StoreError> {
    sqlx::query_as::<_, Goal>("UPDATE goals SET status=?, updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=? RETURNING *")
        .bind(status).bind(id).fetch_optional(pool).await?.ok_or(StoreError::NotFound("Goal not found"))
}
