use serde::Serialize;
use sqlx::SqlitePool;
use uuid::Uuid;

use super::{StoreError, threads};

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct Message {
    pub id: String,
    pub thread_id: String,
    pub role: String,
    pub content: String,
    pub reply_to_id: Option<String>,
    pub created_at: String,
}

pub async fn create(
    pool: &SqlitePool,
    thread_id: &str,
    role: &str,
    content: &str,
    reply_to_id: Option<&str>,
) -> Result<Message, StoreError> {
    let mut transaction = pool.begin().await?;
    let thread = sqlx::query(
        "UPDATE threads SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = ?",
    )
    .bind(thread_id)
    .execute(&mut *transaction)
    .await?;
    if thread.rows_affected() == 0 {
        return Err(StoreError::NotFound("Thread not found"));
    }
    let message = sqlx::query_as::<_, Message>(
        "INSERT INTO messages (id, thread_id, role, content, reply_to_id) VALUES (?, ?, ?, ?, ?) RETURNING id, thread_id, role, content, reply_to_id, created_at",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(thread_id)
    .bind(role)
    .bind(content)
    .bind(reply_to_id)
    .fetch_one(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(message)
}

pub async fn list(
    pool: &SqlitePool,
    thread_id: &str,
    limit: u32,
    offset: u32,
) -> Result<Vec<Message>, StoreError> {
    threads::get(pool, thread_id).await?;
    Ok(sqlx::query_as::<_, Message>(
        "SELECT id, thread_id, role, content, reply_to_id, created_at FROM messages WHERE thread_id = ? ORDER BY sequence LIMIT ? OFFSET ?",
    )
    .bind(thread_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?)
}

pub async fn history_for_reply(
    pool: &SqlitePool,
    thread_id: &str,
    message_id: &str,
) -> Result<Vec<Message>, StoreError> {
    threads::get(pool, thread_id).await?;
    let history = sqlx::query_as::<_, Message>(
        "SELECT id, thread_id, role, content, reply_to_id, created_at FROM messages WHERE thread_id = ? ORDER BY sequence",
    )
    .bind(thread_id)
    .fetch_all(pool)
    .await?;
    let requested = history
        .iter()
        .find(|message| message.id == message_id)
        .ok_or(StoreError::NotFound("Message not found in this thread"))?;
    if requested.role != "user"
        || history.last().map(|message| message.id.as_str()) != Some(message_id)
    {
        return Err(StoreError::Conflict(
            "Replies require the latest unanswered user message",
        ));
    }
    Ok(history)
}
