ALTER TABLE messages ADD COLUMN archived_at TEXT;

CREATE INDEX messages_active_thread_sequence
    ON messages(thread_id, sequence) WHERE archived_at IS NULL;

ALTER TABLE llm_usage RENAME TO llm_usage_old;

CREATE TABLE llm_usage (
    id TEXT PRIMARY KEY NOT NULL,
    thread_id TEXT NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
    message_id TEXT REFERENCES messages(id) ON DELETE SET NULL,
    kind TEXT NOT NULL CHECK (kind IN ('response', 'title', 'compaction')),
    provider TEXT NOT NULL,
    model TEXT NOT NULL,
    prompt_tokens INTEGER CHECK (prompt_tokens IS NULL OR prompt_tokens >= 0),
    completion_tokens INTEGER CHECK (completion_tokens IS NULL OR completion_tokens >= 0),
    total_tokens INTEGER CHECK (total_tokens IS NULL OR total_tokens >= 0),
    cached_input_tokens INTEGER CHECK (cached_input_tokens IS NULL OR cached_input_tokens >= 0),
    cache_creation_input_tokens INTEGER CHECK (cache_creation_input_tokens IS NULL OR cache_creation_input_tokens >= 0),
    usage_json TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

INSERT INTO llm_usage (
    id, thread_id, message_id, kind, provider, model, prompt_tokens,
    completion_tokens, total_tokens, cached_input_tokens,
    cache_creation_input_tokens, usage_json, created_at
)
SELECT
    id, thread_id, message_id, kind, provider, model, prompt_tokens,
    completion_tokens, total_tokens, cached_input_tokens,
    cache_creation_input_tokens, usage_json, created_at
FROM llm_usage_old;

DROP TABLE llm_usage_old;
CREATE INDEX llm_usage_recent ON llm_usage(created_at DESC);
