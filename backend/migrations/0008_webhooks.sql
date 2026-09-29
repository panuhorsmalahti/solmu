CREATE TABLE webhooks (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL CHECK (length(trim(name)) > 0),
    enabled INTEGER NOT NULL DEFAULT 0 CHECK (enabled IN (0, 1)),
    auth_type TEXT NOT NULL CHECK (auth_type IN ('github-hmac-sha256', 'bearer')),
    secret TEXT NOT NULL CHECK (length(secret) >= 16),
    instructions TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE TABLE webhook_deliveries (
    id INTEGER PRIMARY KEY NOT NULL,
    webhook_id TEXT NOT NULL REFERENCES webhooks(id) ON DELETE CASCADE,
    delivery_id TEXT NOT NULL,
    thread_id TEXT REFERENCES threads(id) ON DELETE SET NULL,
    received_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    UNIQUE(webhook_id, delivery_id)
);
