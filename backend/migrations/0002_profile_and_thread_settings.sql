CREATE TABLE profile (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    system_prompt TEXT NOT NULL CHECK (length(trim(system_prompt)) > 0)
);

ALTER TABLE threads ADD COLUMN model TEXT;
ALTER TABLE threads ADD COLUMN workspace TEXT;
