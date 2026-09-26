use super::StoreError;
use serde::Serialize;
use sqlx::SqlitePool;

#[derive(Serialize, sqlx::FromRow)]
pub struct Profile {
    pub system_prompt: String,
    pub model: Option<String>,
    pub edited_at: String,
}

pub async fn get(pool: &SqlitePool) -> Result<Profile, StoreError> {
    Ok(sqlx::query_as::<_, Profile>(
        "SELECT system_prompt, model, edited_at FROM profile WHERE id = 1",
    )
    .fetch_one(pool)
    .await?)
}

pub async fn save(
    pool: &SqlitePool,
    system_prompt: &str,
    model: Option<Option<&str>>,
) -> Result<Profile, StoreError> {
    let mut transaction = pool.begin().await?;
    // Acquire the write lock before reading the current prompt. Both the revision
    // and the current profile commit together, including simultaneous saves.
    sqlx::query("INSERT INTO system_prompt_versions (system_prompt, edited_at) SELECT ?, strftime('%Y-%m-%dT%H:%M:%fZ', 'now') FROM profile WHERE id = 1 AND system_prompt != ?")
        .bind(system_prompt).bind(system_prompt).execute(&mut *transaction).await?;
    let profile = sqlx::query_as::<_, Profile>("UPDATE profile SET edited_at = CASE WHEN system_prompt != ? THEN (SELECT edited_at FROM system_prompt_versions ORDER BY id DESC LIMIT 1) ELSE edited_at END, system_prompt = ?, model = CASE WHEN ? THEN ? ELSE model END WHERE id = 1 RETURNING system_prompt, model, edited_at")
        .bind(system_prompt).bind(system_prompt).bind(model.is_some()).bind(model.flatten())
        .fetch_one(&mut *transaction).await?;
    transaction.commit().await?;
    Ok(profile)
}
