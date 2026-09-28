use super::StoreError;
use genai::{ModelIden, chat::Usage};
use serde::Serialize;
use sqlx::SqlitePool;

#[derive(Clone, Serialize)]
pub struct CacheSummary {
    pub requests: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cached_input_tokens: i64,
    pub cache_creation_input_tokens: i64,
    pub hit_rate_percent: Option<f64>,
}

pub async fn save(
    pool: &SqlitePool,
    thread_id: &str,
    message_id: Option<&str>,
    kind: &str,
    model: &ModelIden,
    usage: &Usage,
) -> Result<(), StoreError> {
    let details = usage.prompt_tokens_details.as_ref();
    let json = serde_json::to_string(usage)
        .map_err(|error| StoreError::Database(sqlx::Error::Decode(Box::new(error))))?;
    sqlx::query("INSERT INTO llm_usage (id,thread_id,message_id,kind,provider,model,prompt_tokens,completion_tokens,total_tokens,cached_input_tokens,cache_creation_input_tokens,usage_json) VALUES (?,?,?,?,?,?,?,?,?,?,?,?)")
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(thread_id).bind(message_id).bind(kind)
        .bind(model.adapter_kind.as_lower_str())
        .bind(model.model_name.split_once("::").map_or(model.model_name.as_str(), |(_, name)| name))
        .bind(usage.prompt_tokens).bind(usage.completion_tokens).bind(usage.total_tokens)
        .bind(details.and_then(|value| value.cached_tokens))
        .bind(details.and_then(|value| value.cache_creation_tokens))
        .bind(json).execute(pool).await?;
    Ok(())
}

pub async fn last_24_hours(pool: &SqlitePool) -> Result<CacheSummary, StoreError> {
    let (requests, input_tokens, output_tokens, cached_input_tokens, cache_creation_input_tokens): (i64, i64, i64, i64, i64) =
        sqlx::query_as("SELECT COUNT(*), COALESCE(SUM(prompt_tokens),0), COALESCE(SUM(completion_tokens),0), COALESCE(SUM(cached_input_tokens),0), COALESCE(SUM(cache_creation_input_tokens),0) FROM llm_usage WHERE created_at >= strftime('%Y-%m-%dT%H:%M:%fZ','now','-24 hours')")
            .fetch_one(pool).await?;
    let hit_rate_percent = (input_tokens > 0)
        .then(|| (cached_input_tokens.min(input_tokens) as f64 / input_tokens as f64) * 100.0);
    Ok(CacheSummary {
        requests,
        input_tokens,
        output_tokens,
        cached_input_tokens,
        cache_creation_input_tokens,
        hit_rate_percent,
    })
}
