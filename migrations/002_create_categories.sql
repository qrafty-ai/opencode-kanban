CREATE TABLE IF NOT EXISTS categories (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    position INTEGER NOT NULL,
    color TEXT,
    created_at TEXT NOT NULL
);
