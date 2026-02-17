CREATE TABLE IF NOT EXISTS repos (
    id TEXT PRIMARY KEY,
    path TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    default_base TEXT,
    remote_url TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
