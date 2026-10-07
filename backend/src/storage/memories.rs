use super::StoreError;
use serde::Serialize;
use sqlx::SqlitePool;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Memory {
    pub id: String,
    pub content: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Serialize)]
pub struct Page {
    pub items: Vec<Memory>,
    pub limit: u32,
    pub offset: u32,
    pub has_more: bool,
}

pub async fn page(pool: &SqlitePool, limit: u32, offset: u32) -> Result<Page, StoreError> {
    let rows = sqlx::query_as::<_, Memory>(
        "SELECT * FROM memories ORDER BY created_at DESC, rowid DESC LIMIT ? OFFSET ?",
    )
    .bind(i64::from(limit) + 1)
    .bind(i64::from(offset))
    .fetch_all(pool)
    .await?;
    let has_more = rows.len() > limit as usize;
    Ok(Page {
        items: rows.into_iter().take(limit as usize).collect(),
        limit,
        offset,
        has_more,
    })
}

pub async fn all(pool: &SqlitePool) -> Result<Vec<Memory>, StoreError> {
    Ok(
        sqlx::query_as::<_, Memory>("SELECT * FROM memories ORDER BY created_at DESC, rowid DESC")
            .fetch_all(pool)
            .await?,
    )
}

pub async fn get(pool: &SqlitePool, id: &str) -> Result<Memory, StoreError> {
    sqlx::query_as::<_, Memory>("SELECT * FROM memories WHERE id=?")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or(StoreError::NotFound("Memory not found"))
}

pub async fn write(pool: &SqlitePool, content: &str) -> Result<Memory, StoreError> {
    Ok(
        sqlx::query_as::<_, Memory>("INSERT INTO memories (id, content) VALUES (?, ?) RETURNING *")
            .bind(Uuid::new_v4().to_string())
            .bind(content.trim())
            .fetch_one(pool)
            .await?,
    )
}

pub async fn update(pool: &SqlitePool, id: &str, content: &str) -> Result<Memory, StoreError> {
    sqlx::query_as::<_, Memory>("UPDATE memories SET content=?, updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=? RETURNING *").bind(content.trim()).bind(id).fetch_optional(pool).await?.ok_or(StoreError::NotFound("Memory not found"))
}

pub async fn delete(pool: &SqlitePool, id: &str) -> Result<(), StoreError> {
    let result = sqlx::query("DELETE FROM memories WHERE id=?")
        .bind(id)
        .execute(pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(StoreError::NotFound("Memory not found"));
    }
    Ok(())
}

pub async fn relevant(pool: &SqlitePool, query: &str) -> Result<Vec<Memory>, StoreError> {
    let query_terms = terms(query);
    if query_terms.is_empty() {
        return Ok(Vec::new());
    }
    let memories = all(pool).await?;
    let mut ranked = memories
        .into_iter()
        .filter_map(|memory| {
            let haystack = terms(&memory.content);
            let score = query_terms
                .iter()
                .filter(|term| haystack.contains(term))
                .count();
            (score > 0).then_some((score, memory))
        })
        .collect::<Vec<_>>();
    ranked.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then_with(|| b.1.created_at.cmp(&a.1.created_at))
    });
    Ok(ranked
        .into_iter()
        .take(5)
        .map(|(_, memory)| memory)
        .collect())
}

fn terms(text: &str) -> Vec<String> {
    let mut terms = Vec::new();
    const STOP_WORDS: &[&str] = &[
        "about", "what", "when", "where", "which", "tell", "please", "does", "have", "with",
        "from", "this", "that", "they", "them", "your", "you", "the", "and", "for", "but", "are",
        "was", "were", "who", "how", "can", "could", "would", "should", "into", "our", "out",
        "not", "just", "like",
    ];
    for mut word in text
        .split(|c: char| !c.is_alphanumeric())
        .map(str::to_lowercase)
    {
        if word.ends_with("ies") && word.len() > 4 {
            word.truncate(word.len() - 3);
            word.push('y');
        } else if word.ends_with('s') && word.len() > 3 {
            word.pop();
        }
        if word.chars().count() >= 3
            && !STOP_WORDS.contains(&word.as_str())
            && !terms.contains(&word)
        {
            terms.push(word);
        }
    }
    terms
}
