CREATE TABLE IF NOT EXISTS account (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    subject_id TEXT NOT NULL,
    email TEXT,
    display_name TEXT,
    status TEXT NOT NULL CHECK (status IN ('connected', 'disconnected')),
    connected_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

INSERT OR IGNORE INTO schema_migrations (version) VALUES (2);