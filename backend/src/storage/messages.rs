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
        "SELECT id, thread_id, role, content, reply_to_id, created_at FROM messages WHERE thread_id = ? AND archived_at IS NULL ORDER BY sequence LIMIT ? OFFSET ?",
    )
    .bind(thread_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?)
}

pub async fn all(pool: &SqlitePool, thread_id: &str) -> Result<Vec<Message>, StoreError> {
    threads::get(pool, thread_id).await?;
    Ok(sqlx::query_as::<_, Message>(
        "SELECT id,thread_id,role,content,reply_to_id,created_at FROM messages WHERE thread_id=? AND archived_at IS NULL ORDER BY sequence",
    )
    .bind(thread_id)
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
        "SELECT id, thread_id, role, content, reply_to_id, created_at FROM messages WHERE thread_id = ? AND archived_at IS NULL ORDER BY sequence",
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

pub async fn compact(
    pool: &SqlitePool,
    thread_id: &str,
    summary: &str,
    keep_message_id: Option<&str>,
) -> Result<Message, StoreError> {
    let summary = summary.trim();
    if summary.is_empty() {
        return Err(StoreError::Conflict("The compacted summary was empty"));
    }
    let mut tx = pool.begin().await?;
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM messages WHERE thread_id=? AND archived_at IS NULL",
    )
    .bind(thread_id)
    .fetch_one(&mut *tx)
    .await?;
    if count < 2 {
        return Err(StoreError::Conflict(
            "There is not enough conversation history to compact",
        ));
    }

    if let Some(keep_id) = keep_message_id {
        let row: Option<(i64, String)> = sqlx::query_as(
            "SELECT sequence, role FROM messages WHERE id=? AND thread_id=? AND archived_at IS NULL",
        )
        .bind(keep_id)
        .bind(thread_id)
        .fetch_optional(&mut *tx)
        .await?;
        let Some((sequence, role)) = row else {
            return Err(StoreError::NotFound("Message to preserve was not found"));
        };
        if role != "user" {
            return Err(StoreError::Conflict("Only a user message can be preserved"));
        }
        sqlx::query("UPDATE messages SET archived_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE thread_id=? AND archived_at IS NULL AND id<>?")
            .bind(thread_id).bind(keep_id).execute(&mut *tx).await?;
        sqlx::query("UPDATE messages SET sequence=? WHERE id=?")
            .bind(-sequence)
            .bind(keep_id)
            .execute(&mut *tx)
            .await?;
    } else {
        sqlx::query("UPDATE messages SET archived_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE thread_id=? AND archived_at IS NULL")
            .bind(thread_id).execute(&mut *tx).await?;
    }

    let thread = sqlx::query(
        "UPDATE threads SET updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?",
    )
    .bind(thread_id)
    .execute(&mut *tx)
    .await?;
    if thread.rows_affected() == 0 {
        return Err(StoreError::NotFound("Thread not found"));
    }
    let message = sqlx::query_as::<_, Message>(
        "INSERT INTO messages (id,thread_id,role,content) VALUES (?,?, 'assistant', ?) RETURNING id,thread_id,role,content,reply_to_id,created_at,sequence",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(thread_id)
    .bind(format!("Conversation summary (compacted):\n\n{summary}"))
    .fetch_one(&mut *tx)
    .await?;
    if let Some(keep_id) = keep_message_id {
        let summary_sequence: i64 = sqlx::query_scalar("SELECT sequence FROM messages WHERE id=?")
            .bind(&message.id)
            .fetch_one(&mut *tx)
            .await?;
        let kept_sequence = summary_sequence + 1;
        sqlx::query("UPDATE messages SET sequence=? WHERE id=?")
            .bind(kept_sequence)
            .bind(keep_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE sqlite_sequence SET seq=? WHERE name='messages'")
            .bind(kept_sequence)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(message)
}
