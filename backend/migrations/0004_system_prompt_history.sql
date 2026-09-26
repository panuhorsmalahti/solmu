ALTER TABLE profile ADD COLUMN edited_at TEXT;

UPDATE profile SET edited_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now');

CREATE TABLE system_prompt_versions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    system_prompt TEXT NOT NULL,
    edited_at TEXT NOT NULL
);

INSERT INTO system_prompt_versions (system_prompt, edited_at)
SELECT system_prompt, edited_at FROM profile WHERE id = 1;
