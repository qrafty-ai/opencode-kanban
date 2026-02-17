CREATE TABLE IF NOT EXISTS tasks (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    repo_id TEXT NOT NULL REFERENCES repos(id),
    branch TEXT NOT NULL,
    category_id TEXT NOT NULL REFERENCES categories(id),
    position INTEGER NOT NULL,
    tmux_session_name TEXT,
    worktree_path TEXT,
    tmux_status TEXT DEFAULT 'unknown',
    status_source TEXT NOT NULL DEFAULT 'none',
    status_fetched_at TEXT,
    status_error TEXT,
    opencode_session_id TEXT,
    session_todo_json TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE(repo_id, branch)
);
