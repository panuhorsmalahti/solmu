CREATE TABLE goals (
    id TEXT PRIMARY KEY NOT NULL,
    objective TEXT NOT NULL CHECK (length(trim(objective)) > 0),
    status TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'paused', 'completed', 'cancelled')),
    thread_id TEXT REFERENCES threads(id) ON DELETE SET NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);
CREATE INDEX goals_status_created ON goals(status, created_at DESC);
