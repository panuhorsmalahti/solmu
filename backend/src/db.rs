use std::{error::Error, str::FromStr, time::Duration};

use sqlx::{
    SqlitePool,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions},
};

pub async fn connect(url: &str) -> Result<SqlitePool, Box<dyn Error>> {
    let options = SqliteConnectOptions::from_str(url)?
        .create_if_missing(true)
        .foreign_keys(true)
        .journal_mode(SqliteJournalMode::Wal)
        .busy_timeout(Duration::from_secs(5));
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await?;
    sqlx::migrate!("./migrations").run(&pool).await?;
    let mut transaction = pool.begin().await?;
    sqlx::query("INSERT OR IGNORE INTO profile (id, system_prompt, edited_at) VALUES (1, ?, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))")
        .bind(crate::prompt::SYSTEM_PROMPT).execute(&mut *transaction).await?;
    sqlx::query("INSERT INTO system_prompt_versions (system_prompt, edited_at) SELECT system_prompt, edited_at FROM profile WHERE id = 1 AND NOT EXISTS (SELECT 1 FROM system_prompt_versions)")
        .execute(&mut *transaction).await?;
    transaction.commit().await?;
    crate::storage::tools::recover(&pool, None)
        .await
        .map_err(|error| format!("Could not recover interrupted tools: {error:?}"))?;
    Ok(pool)
}
