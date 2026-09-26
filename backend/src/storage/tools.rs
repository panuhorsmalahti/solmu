use super::{StoreError, messages::Message, threads};
use crate::tools::ResultData;
use genai::chat::{ChatMessage, ToolResponse};
use serde::Serialize;
use serde_json::Value;
use sqlx::{Row, SqlitePool, sqlite::SqliteRow};

#[derive(Clone, Serialize)]
pub struct ToolRun {
    pub id: String,
    pub thread_id: String,
    pub message_id: String,
    pub call_id: String,
    pub name: String,
    pub arguments: Value,
    pub status: String,
    pub result: Option<Value>,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
}
fn decode(error: serde_json::Error) -> StoreError {
    StoreError::Database(sqlx::Error::Decode(Box::new(error)))
}
fn run(row: SqliteRow) -> Result<ToolRun, StoreError> {
    Ok(ToolRun {
        id: row.try_get("id")?,
        thread_id: row.try_get("thread_id")?,
        message_id: row.try_get("message_id")?,
        call_id: row.try_get("call_id")?,
        name: row.try_get("name")?,
        status: row.try_get("status")?,
        arguments: serde_json::from_str(row.try_get("arguments_json")?).map_err(decode)?,
        result: row
            .try_get::<Option<String>, _>("result_json")?
            .map(|json| serde_json::from_str(&json))
            .transpose()
            .map_err(decode)?,
        started_at: row.try_get("started_at")?,
        finished_at: row.try_get("finished_at")?,
    })
}
pub async fn create_turn(
    pool: &SqlitePool,
    thread_id: &str,
    message_id: &str,
    assistant: &ChatMessage,
) -> Result<Vec<ToolRun>, StoreError> {
    let turn_id = uuid::Uuid::new_v4().to_string();
    let mut tx = pool.begin().await?;
    sqlx::query("INSERT INTO tool_turns (id,thread_id,message_id,assistant_json) VALUES (?,?,?,?)")
        .bind(&turn_id)
        .bind(thread_id)
        .bind(message_id)
        .bind(serde_json::to_string(assistant).map_err(decode)?)
        .execute(&mut *tx)
        .await?;
    let mut runs = Vec::new();
    for call in assistant.content.tool_calls() {
        let row=sqlx::query("INSERT INTO tool_runs (id,turn_id,thread_id,message_id,call_id,name,arguments_json,status) VALUES (?,?,?,?,?,?,?,'queued') RETURNING *")
            .bind(uuid::Uuid::new_v4().to_string()).bind(&turn_id).bind(thread_id).bind(message_id)
            .bind(&call.call_id).bind(&call.fn_name).bind(call.fn_arguments.to_string()).fetch_one(&mut *tx).await?;
        runs.push(run(row)?);
    }
    tx.commit().await?;
    Ok(runs)
}
pub async fn start(pool: &SqlitePool, id: &str) -> Result<ToolRun, StoreError> {
    run(sqlx::query("UPDATE tool_runs SET status='running', started_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=? RETURNING *").bind(id).fetch_one(pool).await?)
}
pub async fn finish(
    pool: &SqlitePool,
    id: &str,
    status: &str,
    result: &ResultData,
) -> Result<ToolRun, StoreError> {
    run(sqlx::query("UPDATE tool_runs SET status=?,result_json=?,finished_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=? RETURNING *")
        .bind(status).bind(serde_json::to_string(result).map_err(decode)?).bind(id).fetch_one(pool).await?)
}
pub async fn recover(pool: &SqlitePool, thread_id: Option<&str>) -> Result<(), StoreError> {
    sqlx::query("UPDATE tool_runs SET status='interrupted',result_json=?,finished_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE status IN ('queued','running') AND (? IS NULL OR thread_id=?)")
        .bind(serde_json::to_string(&ResultData::error("Execution was interrupted; a side effect may already have occurred. Inspect the workspace before retrying.")).map_err(decode)?)
        .bind(thread_id).bind(thread_id).execute(pool).await?;
    Ok(())
}

pub async fn interrupt_runs(pool: &SqlitePool, ids: &[String]) -> Result<(), StoreError> {
    let output = serde_json::to_string(&ResultData::error(
        "Execution was interrupted; inspect the workspace before retrying.",
    ))
    .map_err(decode)?;
    let mut tx = pool.begin().await?;
    for id in ids {
        sqlx::query("UPDATE tool_runs SET status='interrupted',result_json=?,finished_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=? AND status IN ('queued','running')")
            .bind(&output).bind(id).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(())
}
pub async fn list(
    pool: &SqlitePool,
    thread_id: &str,
    limit: u32,
    offset: u32,
) -> Result<Vec<ToolRun>, StoreError> {
    threads::get(pool, thread_id).await?;
    sqlx::query("SELECT * FROM tool_runs WHERE thread_id=? ORDER BY sequence LIMIT ? OFFSET ?")
        .bind(thread_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(run)
        .collect()
}
pub async fn history(
    pool: &SqlitePool,
    thread_id: &str,
    messages: &[Message],
) -> Result<Vec<ChatMessage>, StoreError> {
    let turns = sqlx::query(
        "SELECT id,message_id,assistant_json FROM tool_turns WHERE thread_id=? ORDER BY sequence",
    )
    .bind(thread_id)
    .fetch_all(pool)
    .await?;
    let mut history = Vec::new();
    for message in messages {
        history.push(if message.role == "assistant" {
            ChatMessage::assistant(&message.content)
        } else {
            ChatMessage::user(&message.content)
        });
        if message.role != "user" {
            continue;
        }
        for turn in turns
            .iter()
            .filter(|turn| turn.get::<String, _>("message_id") == message.id)
        {
            history.push(
                serde_json::from_str::<ChatMessage>(turn.get("assistant_json")).map_err(decode)?,
            );
            let results = sqlx::query(
                "SELECT call_id,result_json FROM tool_runs WHERE turn_id=? ORDER BY sequence",
            )
            .bind(turn.get::<String, _>("id"))
            .fetch_all(pool)
            .await?;
            for result in results {
                let output = result
                    .get::<Option<String>, _>("result_json")
                    .unwrap_or_else(|| {
                        serde_json::to_string(&ResultData::error("Execution interrupted"))
                            .expect("tool result")
                    });
                history.push(ToolResponse::new(result.get::<String, _>("call_id"), output).into());
            }
        }
    }
    Ok(history)
}
