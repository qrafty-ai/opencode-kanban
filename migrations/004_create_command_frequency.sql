CREATE TABLE IF NOT EXISTS command_frequency (
    command_id TEXT PRIMARY KEY,
    use_count INTEGER NOT NULL DEFAULT 0,
    last_used TEXT NOT NULL
);
