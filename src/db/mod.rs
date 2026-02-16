#![allow(dead_code)]

use std::{fs, path::Path, process::Command};

use anyhow::{Context, Result, anyhow, bail};
use chrono::Utc;
use rusqlite::{Connection, params, types::Type};
use uuid::Uuid;

use crate::types::{Category, Repo, Task};

const DEFAULT_TMUX_STATUS: &str = "unknown";
const DEFAULT_STATUS_SOURCE: &str = "none";

pub struct Database {
    conn: Connection,
}

impl Database {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path_ref = path.as_ref();

        if path_ref != Path::new(":memory:")
            && let Some(parent) = path_ref.parent()
        {
            fs::create_dir_all(parent).with_context(|| {
                format!(
                    "failed to create parent directories for {}",
                    path_ref.display()
                )
            })?;
        }

        let conn = Connection::open(path_ref)
            .with_context(|| format!("failed to open sqlite db at {}", path_ref.display()))?;

        conn.execute("PRAGMA foreign_keys = ON", params![])
            .context("failed to enable foreign keys")?;

        let db = Self { conn };
        db.run_migrations()?;
        db.seed_default_categories()?;
        Ok(db)
    }

    pub fn add_repo(&self, path: impl AsRef<Path>) -> Result<Repo> {
        let path_buf = fs::canonicalize(path.as_ref()).with_context(|| {
            format!(
                "failed to canonicalize repo path {}",
                path.as_ref().display()
            )
        })?;
        let path_str = path_buf
            .to_str()
            .ok_or_else(|| anyhow!("repo path is not valid UTF-8: {}", path_buf.display()))?
            .to_string();
        let name = derive_repo_name(&path_buf);
        let default_base = detect_default_base(&path_buf);
        let remote_url = detect_remote_url(&path_buf);
        let now = now_iso();
        let id = Uuid::new_v4();

        self.conn
            .execute(
                "INSERT INTO repos (id, path, name, default_base, remote_url, created_at, updated_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    id.to_string(),
                    path_str,
                    name,
                    default_base,
                    remote_url,
                    now,
                    now
                ],
            )
            .context("failed to insert repo")?;

        self.get_repo(id)
    }

    pub fn list_repos(&self) -> Result<Vec<Repo>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, path, name, default_base, remote_url, created_at, updated_at \
             FROM repos ORDER BY created_at ASC",
        )?;

        let repos = stmt
            .query_map(params![], map_repo_row)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .context("failed to load repos")?;

        Ok(repos)
    }

    pub fn add_task(
        &self,
        repo_id: Uuid,
        branch: impl AsRef<str>,
        title: impl AsRef<str>,
        category_id: Uuid,
    ) -> Result<Task> {
        let branch = branch.as_ref().to_string();
        if branch.trim().is_empty() {
            bail!("branch cannot be empty");
        }

        let position: i64 = self.conn.query_row(
            "SELECT COALESCE(MAX(position) + 1, 0) FROM tasks WHERE category_id = ?1",
            params![category_id.to_string()],
            |row| row.get(0),
        )?;

        let title = if title.as_ref().trim().is_empty() {
            let repo_name: String = self.conn.query_row(
                "SELECT name FROM repos WHERE id = ?1",
                params![repo_id.to_string()],
                |row| row.get(0),
            )?;
            format!("{repo_name}:{branch}")
        } else {
            title.as_ref().to_string()
        };

        let now = now_iso();
        let id = Uuid::new_v4();
        self.conn
            .execute(
                "INSERT INTO tasks (
                    id, title, repo_id, branch, category_id, position, tmux_session_name,
                    opencode_session_id, worktree_path, tmux_status, status_source,
                    status_fetched_at, status_error, created_at, updated_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
                params![
                    id.to_string(),
                    title,
                    repo_id.to_string(),
                    branch,
                    category_id.to_string(),
                    position,
                    Option::<String>::None,
                    Option::<String>::None,
                    Option::<String>::None,
                    DEFAULT_TMUX_STATUS,
                    DEFAULT_STATUS_SOURCE,
                    Option::<String>::None,
                    Option::<String>::None,
                    now,
                    now
                ],
            )
            .context("failed to insert task")?;

        self.get_task(id)
    }

    pub fn get_task(&self, id: Uuid) -> Result<Task> {
        self.conn
            .query_row(
                "SELECT id, title, repo_id, branch, category_id, position, tmux_session_name,
                        opencode_session_id, worktree_path, tmux_status, status_source,
                        status_fetched_at, status_error, created_at, updated_at
                 FROM tasks WHERE id = ?1",
                params![id.to_string()],
                map_task_row,
            )
            .with_context(|| format!("task {id} not found"))
    }

    pub fn list_tasks(&self) -> Result<Vec<Task>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, title, repo_id, branch, category_id, position, tmux_session_name,
                    opencode_session_id, worktree_path, tmux_status, status_source,
                    status_fetched_at, status_error, created_at, updated_at
             FROM tasks ORDER BY category_id ASC, position ASC, created_at ASC",
        )?;

        let tasks = stmt
            .query_map(params![], map_task_row)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .context("failed to load tasks")?;
        Ok(tasks)
    }

    pub fn update_task_category(&self, id: Uuid, category_id: Uuid, position: i64) -> Result<()> {
        self.conn
            .execute(
                "UPDATE tasks SET category_id = ?1, position = ?2, updated_at = ?3 WHERE id = ?4",
                params![category_id.to_string(), position, now_iso(), id.to_string()],
            )
            .context("failed to update task category")?;
        Ok(())
    }

    pub fn update_task_position(&self, id: Uuid, position: i64) -> Result<()> {
        self.conn
            .execute(
                "UPDATE tasks SET position = ?1, updated_at = ?2 WHERE id = ?3",
                params![position, now_iso(), id.to_string()],
            )
            .context("failed to update task position")?;
        Ok(())
    }

    pub fn update_task_tmux(
        &self,
        id: Uuid,
        tmux_session_name: Option<String>,
        opencode_session_id: Option<String>,
        worktree_path: Option<String>,
    ) -> Result<()> {
        self.conn
            .execute(
                "UPDATE tasks
                 SET tmux_session_name = ?1,
                     opencode_session_id = ?2,
                     worktree_path = ?3,
                     updated_at = ?4
                 WHERE id = ?5",
                params![
                    tmux_session_name,
                    opencode_session_id,
                    worktree_path,
                    now_iso(),
                    id.to_string()
                ],
            )
            .context("failed to update task tmux metadata")?;
        Ok(())
    }

    pub fn update_task_status(&self, id: Uuid, status: impl AsRef<str>) -> Result<()> {
        self.conn
            .execute(
                "UPDATE tasks SET tmux_status = ?1, updated_at = ?2 WHERE id = ?3",
                params![status.as_ref(), now_iso(), id.to_string()],
            )
            .context("failed to update task status")?;
        Ok(())
    }

    pub fn update_task_status_metadata(
        &self,
        id: Uuid,
        status_source: impl AsRef<str>,
        status_fetched_at: Option<String>,
        status_error: Option<String>,
    ) -> Result<()> {
        self.conn
            .execute(
                "UPDATE tasks
                 SET status_source = ?1,
                     status_fetched_at = ?2,
                     status_error = ?3,
                     updated_at = ?4
                 WHERE id = ?5",
                params![
                    status_source.as_ref(),
                    status_fetched_at,
                    status_error,
                    now_iso(),
                    id.to_string()
                ],
            )
            .context("failed to update task status metadata")?;
        Ok(())
    }

    pub fn delete_task(&self, id: Uuid) -> Result<()> {
        self.conn
            .execute("DELETE FROM tasks WHERE id = ?1", params![id.to_string()])
            .context("failed to delete task")?;
        Ok(())
    }

    pub fn add_category(&self, name: impl AsRef<str>, position: i64) -> Result<Category> {
        let now = now_iso();
        let id = Uuid::new_v4();
        self.conn
            .execute(
                "INSERT INTO categories (id, name, position, created_at) VALUES (?1, ?2, ?3, ?4)",
                params![id.to_string(), name.as_ref(), position, now],
            )
            .context("failed to insert category")?;

        self.get_category(id)
    }

    pub fn list_categories(&self) -> Result<Vec<Category>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, position, created_at FROM categories ORDER BY position ASC",
        )?;

        let categories = stmt
            .query_map(params![], map_category_row)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .context("failed to load categories")?;

        Ok(categories)
    }

    pub fn update_category_position(&self, id: Uuid, position: i64) -> Result<()> {
        self.conn
            .execute(
                "UPDATE categories SET position = ?1 WHERE id = ?2",
                params![position, id.to_string()],
            )
            .context("failed to update category position")?;
        Ok(())
    }

    pub fn rename_category(&self, id: Uuid, name: impl AsRef<str>) -> Result<()> {
        self.conn
            .execute(
                "UPDATE categories SET name = ?1 WHERE id = ?2",
                params![name.as_ref(), id.to_string()],
            )
            .context("failed to rename category")?;
        Ok(())
    }

    pub fn delete_category(&self, id: Uuid) -> Result<()> {
        self.conn
            .execute(
                "DELETE FROM categories WHERE id = ?1",
                params![id.to_string()],
            )
            .context("failed to delete category")?;
        Ok(())
    }

    fn run_migrations(&self) -> Result<()> {
        self.conn
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS repos (
                    id TEXT PRIMARY KEY,
                    path TEXT NOT NULL UNIQUE,
                    name TEXT NOT NULL,
                    default_base TEXT,
                    remote_url TEXT,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                );

                CREATE TABLE IF NOT EXISTS categories (
                    id TEXT PRIMARY KEY,
                    name TEXT NOT NULL UNIQUE,
                    position INTEGER NOT NULL,
                    created_at TEXT NOT NULL
                );

                CREATE TABLE IF NOT EXISTS tasks (
                    id TEXT PRIMARY KEY,
                    title TEXT NOT NULL,
                    repo_id TEXT NOT NULL REFERENCES repos(id),
                    branch TEXT NOT NULL,
                    category_id TEXT NOT NULL REFERENCES categories(id),
                    position INTEGER NOT NULL,
                    tmux_session_name TEXT,
                    opencode_session_id TEXT,
                    worktree_path TEXT,
                    tmux_status TEXT DEFAULT 'unknown',
                    status_source TEXT NOT NULL DEFAULT 'none',
                    status_fetched_at TEXT,
                    status_error TEXT,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL,
                    UNIQUE(repo_id, branch)
                );",
            )
            .context("failed to run sqlite migrations")?;

        self.conn
            .execute(
                "ALTER TABLE tasks ADD COLUMN status_source TEXT NOT NULL DEFAULT 'none'",
                params![],
            )
            .or_else(|err| {
                if is_duplicate_column_err(&err) {
                    Ok(0)
                } else {
                    Err(err)
                }
            })
            .context("failed to migrate tasks.status_source")?;

        self.conn
            .execute(
                "ALTER TABLE tasks ADD COLUMN status_fetched_at TEXT",
                params![],
            )
            .or_else(|err| {
                if is_duplicate_column_err(&err) {
                    Ok(0)
                } else {
                    Err(err)
                }
            })
            .context("failed to migrate tasks.status_fetched_at")?;

        self.conn
            .execute("ALTER TABLE tasks ADD COLUMN status_error TEXT", params![])
            .or_else(|err| {
                if is_duplicate_column_err(&err) {
                    Ok(0)
                } else {
                    Err(err)
                }
            })
            .context("failed to migrate tasks.status_error")?;

        self.conn
            .execute(
                "UPDATE tasks SET status_source = 'none' WHERE status_source IS NULL",
                params![],
            )
            .context("failed to backfill tasks.status_source")?;

        Ok(())
    }

    fn seed_default_categories(&self) -> Result<()> {
        let mut stmt = self
            .conn
            .prepare("SELECT COUNT(*) FROM categories")
            .context("failed to prepare category count query")?;
        let category_count: i64 = stmt.query_row(params![], |row| row.get(0))?;

        if category_count == 0 {
            self.add_category("TODO", 0)?;
            self.add_category("IN PROGRESS", 1)?;
            self.add_category("DONE", 2)?;
        }

        Ok(())
    }

    fn get_repo(&self, id: Uuid) -> Result<Repo> {
        self.conn
            .query_row(
                "SELECT id, path, name, default_base, remote_url, created_at, updated_at
                 FROM repos WHERE id = ?1",
                params![id.to_string()],
                map_repo_row,
            )
            .with_context(|| format!("repo {id} not found"))
    }

    fn get_category(&self, id: Uuid) -> Result<Category> {
        self.conn
            .query_row(
                "SELECT id, name, position, created_at FROM categories WHERE id = ?1",
                params![id.to_string()],
                map_category_row,
            )
            .with_context(|| format!("category {id} not found"))
    }
}

fn map_repo_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Repo> {
    Ok(Repo {
        id: parse_uuid_column(row.get::<_, String>(0)?, 0)?,
        path: row.get(1)?,
        name: row.get(2)?,
        default_base: row.get(3)?,
        remote_url: row.get(4)?,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
    })
}

fn map_category_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Category> {
    Ok(Category {
        id: parse_uuid_column(row.get::<_, String>(0)?, 0)?,
        name: row.get(1)?,
        position: row.get(2)?,
        created_at: row.get(3)?,
    })
}

fn map_task_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Task> {
    Ok(Task {
        id: parse_uuid_column(row.get::<_, String>(0)?, 0)?,
        title: row.get(1)?,
        repo_id: parse_uuid_column(row.get::<_, String>(2)?, 2)?,
        branch: row.get(3)?,
        category_id: parse_uuid_column(row.get::<_, String>(4)?, 4)?,
        position: row.get(5)?,
        tmux_session_name: row.get(6)?,
        opencode_session_id: row.get(7)?,
        worktree_path: row.get(8)?,
        tmux_status: row.get(9)?,
        status_source: row.get(10)?,
        status_fetched_at: row.get(11)?,
        status_error: row.get(12)?,
        created_at: row.get(13)?,
        updated_at: row.get(14)?,
    })
}

fn is_duplicate_column_err(err: &rusqlite::Error) -> bool {
    matches!(
        err,
        rusqlite::Error::SqliteFailure(_, Some(msg)) if msg.contains("duplicate column name")
    )
}

fn parse_uuid_column(value: String, idx: usize) -> rusqlite::Result<Uuid> {
    Uuid::parse_str(&value)
        .map_err(|err| rusqlite::Error::FromSqlConversionFailure(idx, Type::Text, Box::new(err)))
}

fn now_iso() -> String {
    Utc::now().to_rfc3339()
}

fn derive_repo_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| path.to_string_lossy().to_string())
}

fn detect_default_base(path: &Path) -> Option<String> {
    run_git(path, ["symbolic-ref", "refs/remotes/origin/HEAD"])
        .and_then(|raw| raw.strip_prefix("refs/remotes/origin/").map(str::to_string))
}

fn detect_remote_url(path: &Path) -> Option<String> {
    run_git(path, ["remote", "get-url", "origin"])
}

fn run_git<const N: usize>(path: &Path, args: [&str; N]) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(args)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8(output.stdout).ok()?;
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

#[cfg(test)]
mod tests {
    use std::{path::PathBuf, process::Command};

    use anyhow::Result;
    use rusqlite::{Connection, params};
    use uuid::Uuid;

    use super::Database;

    #[test]
    fn test_db_creation_seeds_default_categories() -> Result<()> {
        let db = Database::open(":memory:")?;
        let categories = db.list_categories()?;

        assert_eq!(categories.len(), 3);
        assert_eq!(categories[0].name, "TODO");
        assert_eq!(categories[0].position, 0);
        assert_eq!(categories[1].name, "IN PROGRESS");
        assert_eq!(categories[1].position, 1);
        assert_eq!(categories[2].name, "DONE");
        assert_eq!(categories[2].position, 2);

        Ok(())
    }

    #[test]
    fn test_open_creates_database_file() -> Result<()> {
        let path = temp_path("sqlite-file").join("opencode-kanban.sqlite");
        let _db = Database::open(&path)?;
        assert!(path.exists());

        if let Some(parent) = path.parent() {
            std::fs::remove_dir_all(parent)?;
        }

        Ok(())
    }

    #[test]
    fn test_repo_crud() -> Result<()> {
        let db = Database::open(":memory:")?;
        let repo_dir = create_temp_git_repo("repo-crud")?;

        let repo = db.add_repo(&repo_dir)?;
        assert!(repo.name.starts_with("opencode-kanban-repo-crud-"));
        assert_eq!(repo.default_base.as_deref(), Some("main"));
        assert_eq!(
            repo.remote_url.as_deref(),
            Some("https://example.com/repo-crud.git")
        );

        let repos = db.list_repos()?;
        assert_eq!(repos.len(), 1);
        assert_eq!(repos[0].id, repo.id);

        std::fs::remove_dir_all(&repo_dir)?;
        Ok(())
    }

    #[test]
    fn test_task_crud() -> Result<()> {
        let db = Database::open(":memory:")?;
        let repo_dir = create_temp_git_repo("task-crud")?;
        let repo = db.add_repo(&repo_dir)?;
        let categories = db.list_categories()?;
        let todo_category = categories[0].id;
        let done_category = categories[2].id;

        let task = db.add_task(repo.id, "feature/db-layer", "", todo_category)?;
        assert!(
            task.title.starts_with("opencode-kanban-task-crud-")
                && task.title.ends_with(":feature/db-layer")
        );
        assert_eq!(task.position, 0);
        assert_eq!(task.tmux_status, "unknown");
        assert_eq!(task.status_source, "none");
        assert_eq!(task.status_fetched_at, None);
        assert_eq!(task.status_error, None);

        let fetched = db.get_task(task.id)?;
        assert_eq!(fetched.id, task.id);

        db.update_task_position(task.id, 5)?;
        db.update_task_category(task.id, done_category, 1)?;
        db.update_task_tmux(
            task.id,
            Some("ok-task-crud-feature-db-layer".to_string()),
            Some(Uuid::new_v4().to_string()),
            Some("/tmp/task-crud-feature-db-layer".to_string()),
        )?;
        db.update_task_status(task.id, "running")?;
        db.update_task_status_metadata(
            task.id,
            "tmux",
            Some("2026-02-15T12:34:56Z".to_string()),
            Some("transient timeout".to_string()),
        )?;

        let updated = db.get_task(task.id)?;
        assert_eq!(updated.position, 1);
        assert_eq!(updated.category_id, done_category);
        assert_eq!(updated.tmux_status, "running");
        assert_eq!(updated.status_source, "tmux");
        assert_eq!(
            updated.status_fetched_at.as_deref(),
            Some("2026-02-15T12:34:56Z")
        );
        assert_eq!(updated.status_error.as_deref(), Some("transient timeout"));
        assert_eq!(
            updated.tmux_session_name.as_deref(),
            Some("ok-task-crud-feature-db-layer")
        );

        let tasks = db.list_tasks()?;
        assert_eq!(tasks.len(), 1);

        db.delete_task(task.id)?;
        assert!(db.get_task(task.id).is_err());

        std::fs::remove_dir_all(&repo_dir)?;
        Ok(())
    }

    #[test]
    fn test_category_crud() -> Result<()> {
        let db = Database::open(":memory:")?;

        let category = db.add_category("REVIEW", 3)?;
        db.rename_category(category.id, "QA")?;
        db.update_category_position(category.id, 4)?;

        let categories = db.list_categories()?;
        let qa = categories
            .into_iter()
            .find(|c| c.id == category.id)
            .unwrap();
        assert_eq!(qa.name, "QA");
        assert_eq!(qa.position, 4);

        db.delete_category(category.id)?;
        let categories = db.list_categories()?;
        assert!(!categories.iter().any(|c| c.id == category.id));

        Ok(())
    }

    #[test]
    fn test_categories_reorder_positions() -> Result<()> {
        let db = Database::open(":memory:")?;
        let categories = db.list_categories()?;
        assert_eq!(categories.len(), 3);

        let first = categories[0].id;
        let third = categories[2].id;

        db.update_category_position(first, 2)?;
        db.update_category_position(third, 0)?;

        let reordered = db.list_categories()?;
        assert_eq!(reordered[0].id, third);
        assert_eq!(reordered[2].id, first);

        Ok(())
    }

    #[test]
    fn test_categories_reorder_tasks_within_category() -> Result<()> {
        let db = Database::open(":memory:")?;
        let repo_dir = create_temp_git_repo("task-reorder")?;
        let repo = db.add_repo(&repo_dir)?;
        let todo_category = db.list_categories()?[0].id;

        let first = db.add_task(repo.id, "feature/reorder-1", "First", todo_category)?;
        let second = db.add_task(repo.id, "feature/reorder-2", "Second", todo_category)?;

        db.update_task_position(first.id, 1)?;
        db.update_task_position(second.id, 0)?;

        let mut ordered: Vec<_> = db
            .list_tasks()?
            .into_iter()
            .filter(|task| task.category_id == todo_category)
            .collect();
        ordered.sort_by_key(|task| task.position);

        assert_eq!(ordered[0].id, second.id);
        assert_eq!(ordered[1].id, first.id);

        std::fs::remove_dir_all(&repo_dir)?;
        Ok(())
    }

    #[test]
    fn test_duplicate_repo_branch() -> Result<()> {
        let db = Database::open(":memory:")?;
        let repo_dir = create_temp_git_repo("dupe-branch")?;
        let repo = db.add_repo(&repo_dir)?;
        let todo_category = db.list_categories()?[0].id;

        let _task = db.add_task(repo.id, "same-branch", "Task One", todo_category)?;
        let duplicate = db.add_task(repo.id, "same-branch", "Task Two", todo_category);
        assert!(duplicate.is_err());

        std::fs::remove_dir_all(&repo_dir)?;
        Ok(())
    }

    #[test]
    fn test_unique_constraints_and_foreign_key_behavior() -> Result<()> {
        let db = Database::open(":memory:")?;
        let repo_dir = create_temp_git_repo("constraints")?;
        let repo = db.add_repo(&repo_dir)?;
        let duplicate_repo = db.add_repo(&repo_dir);
        assert!(duplicate_repo.is_err());

        let duplicate_category = db.add_category("TODO", 99);
        assert!(duplicate_category.is_err());

        let todo_category = db.list_categories()?[0].id;
        let _task = db.add_task(repo.id, "fk-branch", "FK Task", todo_category)?;

        let delete_in_use_category = db.delete_category(todo_category);
        assert!(delete_in_use_category.is_err());

        std::fs::remove_dir_all(&repo_dir)?;
        Ok(())
    }

    #[test]
    fn test_migration_adds_status_metadata_columns_for_existing_db() -> Result<()> {
        let path = temp_path("migration-status-metadata").join("opencode-kanban.sqlite");
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let conn = Connection::open(&path)?;
        conn.execute_batch(
            "PRAGMA foreign_keys = ON;
             CREATE TABLE repos (
                id TEXT PRIMARY KEY,
                path TEXT NOT NULL UNIQUE,
                name TEXT NOT NULL,
                default_base TEXT,
                remote_url TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
             );
             CREATE TABLE categories (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL UNIQUE,
                position INTEGER NOT NULL,
                created_at TEXT NOT NULL
             );
             CREATE TABLE tasks (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                repo_id TEXT NOT NULL REFERENCES repos(id),
                branch TEXT NOT NULL,
                category_id TEXT NOT NULL REFERENCES categories(id),
                position INTEGER NOT NULL,
                tmux_session_name TEXT,
                opencode_session_id TEXT,
                worktree_path TEXT,
                tmux_status TEXT DEFAULT 'unknown',
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                UNIQUE(repo_id, branch)
             );",
        )?;

        let repo_id = Uuid::new_v4();
        let category_id = Uuid::new_v4();
        let task_id = Uuid::new_v4();
        conn.execute(
            "INSERT INTO repos (id, path, name, default_base, remote_url, created_at, updated_at)
             VALUES (?1, ?2, ?3, NULL, NULL, ?4, ?4)",
            params![
                repo_id.to_string(),
                "/tmp/legacy-repo",
                "legacy-repo",
                "2026-02-15T00:00:00Z"
            ],
        )?;
        conn.execute(
            "INSERT INTO categories (id, name, position, created_at) VALUES (?1, ?2, 0, ?3)",
            params![category_id.to_string(), "TODO", "2026-02-15T00:00:00Z"],
        )?;
        conn.execute(
            "INSERT INTO tasks (
                id, title, repo_id, branch, category_id, position, tmux_session_name,
                opencode_session_id, worktree_path, tmux_status, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, 0, NULL, NULL, NULL, ?6, ?7, ?7)",
            params![
                task_id.to_string(),
                "legacy task",
                repo_id.to_string(),
                "feature/legacy",
                category_id.to_string(),
                "running",
                "2026-02-15T00:00:00Z"
            ],
        )?;
        drop(conn);

        let db = Database::open(&path)?;
        let migrated_task = db.get_task(task_id)?;
        assert_eq!(migrated_task.tmux_status, "running");
        assert_eq!(migrated_task.status_source, "none");
        assert_eq!(migrated_task.status_fetched_at, None);
        assert_eq!(migrated_task.status_error, None);

        let status_source_type: String = db.conn.query_row(
            "SELECT type FROM pragma_table_info('tasks') WHERE name = 'status_source'",
            params![],
            |row| row.get(0),
        )?;
        assert_eq!(status_source_type, "TEXT");

        db.update_task_status(task_id, "dead")?;
        db.update_task_status_metadata(task_id, "server", None, None)?;
        let updated = db.get_task(task_id)?;
        assert_eq!(updated.tmux_status, "dead");
        assert_eq!(updated.status_source, "server");

        if let Some(parent) = path.parent() {
            std::fs::remove_dir_all(parent)?;
        }

        Ok(())
    }

    fn create_temp_git_repo(name: &str) -> Result<PathBuf> {
        let repo_dir = temp_path(name);
        std::fs::create_dir_all(&repo_dir)?;

        run_git_cmd(&repo_dir, &["init", "-b", "main"])?;
        run_git_cmd(
            &repo_dir,
            &[
                "remote",
                "add",
                "origin",
                &format!("https://example.com/{name}.git"),
            ],
        )?;
        run_git_cmd(
            &repo_dir,
            &[
                "symbolic-ref",
                "refs/remotes/origin/HEAD",
                "refs/remotes/origin/main",
            ],
        )?;

        Ok(repo_dir)
    }

    fn run_git_cmd(repo_dir: &PathBuf, args: &[&str]) -> Result<()> {
        let output = Command::new("git")
            .arg("-C")
            .arg(repo_dir)
            .args(args)
            .output()?;
        if !output.status.success() {
            anyhow::bail!(
                "git command failed: git -C {} {}\nstdout: {}\nstderr: {}",
                repo_dir.display(),
                args.join(" "),
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        Ok(())
    }

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("opencode-kanban-{name}-{}", Uuid::new_v4()))
    }
}
