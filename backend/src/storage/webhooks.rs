use serde::Serialize;
use sqlx::SqlitePool;
use uuid::Uuid;

use super::StoreError;

#[derive(Clone, Serialize, sqlx::FromRow)]
pub struct Webhook {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub auth_type: String,
    #[serde(skip_serializing)]
    pub secret: String,
    pub instructions: String,
    pub created_at: String,
    pub updated_at: String,
}

pub async fn list(pool: &SqlitePool) -> Result<Vec<Webhook>, StoreError> {
    Ok(
        sqlx::query_as("SELECT * FROM webhooks ORDER BY created_at DESC, id")
            .fetch_all(pool)
            .await?,
    )
}

pub async fn get(pool: &SqlitePool, id: &str) -> Result<Webhook, StoreError> {
    sqlx::query_as("SELECT * FROM webhooks WHERE id=?")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or(StoreError::NotFound("Webhook not found"))
}

pub async fn create(
    pool: &SqlitePool,
    name: &str,
    auth_type: &str,
    secret: &str,
    instructions: &str,
) -> Result<Webhook, StoreError> {
    Ok(sqlx::query_as("INSERT INTO webhooks (id,name,auth_type,secret,instructions) VALUES (?,?,?,?,?) RETURNING *")
        .bind(Uuid::new_v4().to_string()).bind(name).bind(auth_type).bind(secret).bind(instructions)
        .fetch_one(pool).await?)
}

pub async fn update(
    pool: &SqlitePool,
    id: &str,
    name: &str,
    enabled: bool,
    auth_type: &str,
    secret: &str,
    instructions: &str,
) -> Result<Webhook, StoreError> {
    sqlx::query_as("UPDATE webhooks SET name=?,enabled=?,auth_type=?,secret=?,instructions=?,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=? RETURNING *")
        .bind(name).bind(enabled).bind(auth_type).bind(secret).bind(instructions).bind(id)
        .fetch_optional(pool).await?.ok_or(StoreError::NotFound("Webhook not found"))
}

pub async fn delete(pool: &SqlitePool, id: &str) -> Result<(), StoreError> {
    let result = sqlx::query("DELETE FROM webhooks WHERE id=?")
        .bind(id)
        .execute(pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(StoreError::NotFound("Webhook not found"));
    }
    Ok(())
}

pub async fn claim_delivery(
    pool: &SqlitePool,
    webhook_id: &str,
    delivery_id: &str,
    name: &str,
    workspace: &str,
    content: &str,
) -> Result<Option<(String, String)>, StoreError> {
    let mut transaction = pool.begin().await?;
    let thread_id = Uuid::new_v4().to_string();
    let message_id = Uuid::new_v4().to_string();
    let claimed = sqlx::query(
        "INSERT OR IGNORE INTO webhook_deliveries (webhook_id,delivery_id) VALUES (?,?)",
    )
    .bind(webhook_id)
    .bind(delivery_id)
    .execute(&mut *transaction)
    .await?;
    if claimed.rows_affected() == 0 {
        transaction.rollback().await?;
        return Ok(None);
    }
    sqlx::query("INSERT INTO threads (id,title,workspace) VALUES (?,?,?)")
        .bind(&thread_id)
        .bind(format!("{name} webhook"))
        .bind(workspace)
        .execute(&mut *transaction)
        .await?;
    sqlx::query("INSERT INTO messages (id,thread_id,role,content) VALUES (?,?,'user',?)")
        .bind(&message_id)
        .bind(&thread_id)
        .bind(content)
        .execute(&mut *transaction)
        .await?;
    sqlx::query("UPDATE webhook_deliveries SET thread_id=? WHERE webhook_id=? AND delivery_id=?")
        .bind(&thread_id)
        .bind(webhook_id)
        .bind(delivery_id)
        .execute(&mut *transaction)
        .await?;
    transaction.commit().await?;
    Ok(Some((thread_id, message_id)))
}
