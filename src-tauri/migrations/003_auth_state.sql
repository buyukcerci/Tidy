ALTER TABLE account ADD COLUMN auth_state TEXT NOT NULL DEFAULT 'idle'
    CHECK (auth_state IN ('idle', 'connected', 'reauthentication_required'));
ALTER TABLE account ADD COLUMN auth_message TEXT;

INSERT INTO schema_migrations (version) VALUES (3);
