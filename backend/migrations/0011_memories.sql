CREATE TABLE memories (
    id TEXT PRIMARY KEY NOT NULL,
    content TEXT NOT NULL CHECK (length(trim(content)) > 0),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);
CREATE INDEX memories_created ON memories(created_at DESC, id DESC);
