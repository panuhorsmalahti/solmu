use serde::Serialize;
use sqlx::SqlitePool;
use uuid::Uuid;

use super::StoreError;

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct Thread {
    pub id: String,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
}

pub async fn create(pool: &SqlitePool, title: &str) -> Result<Thread, StoreError> {
    Ok(
        sqlx::query_as::<_, Thread>("INSERT INTO threads (id, title) VALUES (?, ?) RETURNING *")
            .bind(Uuid::new_v4().to_string())
            .bind(title)
            .fetch_one(pool)
            .await?,
    )
}

pub async fn list(pool: &SqlitePool, limit: u32, offset: u32) -> Result<Vec<Thread>, StoreError> {
    Ok(sqlx::query_as::<_, Thread>(
        "SELECT * FROM threads ORDER BY updated_at DESC, id LIMIT ? OFFSET ?",
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?)
}

pub async fn get(pool: &SqlitePool, id: &str) -> Result<Thread, StoreError> {
    sqlx::query_as::<_, Thread>("SELECT * FROM threads WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or(StoreError::NotFound("Thread not found"))
}

pub async fn update(pool: &SqlitePool, id: &str, title: &str) -> Result<Thread, StoreError> {
    sqlx::query_as::<_, Thread>(
        "UPDATE threads SET title = ?, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = ? RETURNING *",
    )
    .bind(title)
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or(StoreError::NotFound("Thread not found"))
}

pub async fn delete(pool: &SqlitePool, id: &str) -> Result<(), StoreError> {
    let result = sqlx::query("DELETE FROM threads WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(StoreError::NotFound("Thread not found"));
    }
    Ok(())
}
