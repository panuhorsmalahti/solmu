CREATE TABLE scheduled_tasks (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL CHECK (length(trim(name)) > 0),
    prompt TEXT NOT NULL CHECK (length(trim(prompt)) > 0),
    schedule_kind TEXT NOT NULL CHECK (schedule_kind IN ('once', 'cron')),
    schedule TEXT NOT NULL,
    thread_id TEXT NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
    enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
    running INTEGER NOT NULL DEFAULT 0 CHECK (running IN (0, 1)),
    next_run_at TEXT,
    last_run_at TEXT,
    last_status TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);
CREATE INDEX scheduled_tasks_due ON scheduled_tasks(enabled, running, next_run_at);

CREATE TABLE scheduled_task_runs (
    id TEXT PRIMARY KEY NOT NULL,
    task_id TEXT NOT NULL REFERENCES scheduled_tasks(id) ON DELETE CASCADE,
    scheduled_for TEXT NOT NULL,
    started_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    finished_at TEXT,
    status TEXT NOT NULL CHECK (status IN ('running', 'completed', 'failed', 'interrupted')),
    error TEXT,
    message_id TEXT REFERENCES messages(id) ON DELETE SET NULL
);
CREATE INDEX scheduled_task_runs_task_started ON scheduled_task_runs(task_id, started_at DESC);
