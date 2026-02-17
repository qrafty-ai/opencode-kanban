#![allow(dead_code)]

use std::{collections::HashMap, fs, future::Future, path::Path, process::Command, str::FromStr};

use anyhow::{Context, Result, anyhow, bail};
use chrono::Utc;
use sqlx::{
    Row,
    sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions, SqliteRow},
};
use tokio::runtime::{Builder, Runtime};
use uuid::Uuid;

use crate::types::{Category, CommandFrequency, Repo, Task};

const DEFAULT_TMUX_STATUS: &str = "unknown";
const DEFAULT_STATUS_SOURCE: &str = "none";

pub struct Database {
    pool: SqlitePool,
    runtime: Runtime,
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

        let runtime = Builder::new_current_thread()
            .enable_all()
            .build()
            .context("failed to build tokio runtime for database")?;

        let connect_options = if path_ref == Path::new(":memory:") {
            SqliteConnectOptions::from_str("sqlite::memory:")
                .context("failed to build sqlite in-memory options")?
        } else {
            SqliteConnectOptions::new()
                .filename(path_ref)
                .create_if_missing(true)
        }
        .foreign_keys(true);

        let pool = runtime
            .block_on(async {
                SqlitePoolOptions::new()
                    .max_connections(1)
                    .connect_with(connect_options)
                    .await
            })
            .with_context(|| format!("failed to open sqlite db at {}", path_ref.display()))?;

        let db = Self { pool, runtime };
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

        self.block_on(async {
            sqlx::query(
                "INSERT INTO repos (id, path, name, default_base, remote_url, created_at, updated_at) \
                 VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(id.to_string())
            .bind(path_str)
            .bind(name)
            .bind(default_base)
            .bind(remote_url)
            .bind(now.clone())
            .bind(now)
            .execute(&self.pool)
            .await
            .context("failed to insert repo")?;

            self.get_repo_async(id).await
        })
    }

    pub fn list_repos(&self) -> Result<Vec<Repo>> {
        self.block_on(async {
            let rows = sqlx::query(
                "SELECT id, path, name, default_base, remote_url, created_at, updated_at \
                 FROM repos ORDER BY created_at ASC",
            )
            .fetch_all(&self.pool)
            .await
            .context("failed to load repos")?;

            rows.into_iter()
                .map(map_repo_row)
                .collect::<Result<Vec<_>>>()
        })
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

        self.block_on(async {
            let mut tx = self
                .pool
                .begin()
                .await
                .context("failed to start add_task transaction")?;

            let position: i64 = sqlx::query_scalar(
                "SELECT COALESCE(MAX(position) + 1, 0) FROM tasks WHERE category_id = ?",
            )
            .bind(category_id.to_string())
            .fetch_one(&mut *tx)
            .await
            .context("failed to resolve next task position")?;

            let title = if title.as_ref().trim().is_empty() {
                let repo_name: String = sqlx::query_scalar("SELECT name FROM repos WHERE id = ?")
                    .bind(repo_id.to_string())
                    .fetch_one(&mut *tx)
                    .await
                    .context("failed to resolve repo name for task title")?;
                format!("{repo_name}:{branch}")
            } else {
                title.as_ref().to_string()
            };

            let now = now_iso();
            let id = Uuid::new_v4();

            sqlx::query(
                "INSERT INTO tasks (
                    id, title, repo_id, branch, category_id, position, tmux_session_name,
                    worktree_path, tmux_status, status_source,
                    status_fetched_at, status_error, opencode_session_id, session_todo_json,
                    created_at, updated_at
                 ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(id.to_string())
            .bind(title)
            .bind(repo_id.to_string())
            .bind(branch)
            .bind(category_id.to_string())
            .bind(position)
            .bind(None::<String>)
            .bind(None::<String>)
            .bind(DEFAULT_TMUX_STATUS)
            .bind(DEFAULT_STATUS_SOURCE)
            .bind(None::<String>)
            .bind(None::<String>)
            .bind(None::<String>)
            .bind(None::<String>)
            .bind(now.clone())
            .bind(now)
            .execute(&mut *tx)
            .await
            .context("failed to insert task")?;

            tx.commit()
                .await
                .context("failed to commit add_task transaction")?;

            self.get_task_async(id).await
        })
    }

    pub fn get_task(&self, id: Uuid) -> Result<Task> {
        self.block_on(self.get_task_async(id))
    }

    pub fn list_tasks(&self) -> Result<Vec<Task>> {
        self.block_on(async {
            let rows = sqlx::query(
                "SELECT id, title, repo_id, branch, category_id, position, tmux_session_name,
                        worktree_path, tmux_status, status_source,
                        status_fetched_at, status_error, opencode_session_id, session_todo_json,
                        created_at, updated_at
                 FROM tasks ORDER BY category_id ASC, position ASC, created_at ASC",
            )
            .fetch_all(&self.pool)
            .await
            .context("failed to load tasks")?;

            rows.into_iter()
                .map(map_task_row)
                .collect::<Result<Vec<_>>>()
        })
    }

    pub fn update_task_category(&self, id: Uuid, category_id: Uuid, position: i64) -> Result<()> {
        self.block_on(async {
            sqlx::query(
                "UPDATE tasks SET category_id = ?, position = ?, updated_at = ? WHERE id = ?",
            )
            .bind(category_id.to_string())
            .bind(position)
            .bind(now_iso())
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .context("failed to update task category")?;
            Ok(())
        })
    }

    pub fn update_task_position(&self, id: Uuid, position: i64) -> Result<()> {
        self.block_on(async {
            sqlx::query("UPDATE tasks SET position = ?, updated_at = ? WHERE id = ?")
                .bind(position)
                .bind(now_iso())
                .bind(id.to_string())
                .execute(&self.pool)
                .await
                .context("failed to update task position")?;
            Ok(())
        })
    }

    pub fn reorder_task_positions(&self, positions: &[(Uuid, i64)]) -> Result<()> {
        self.block_on(async {
            let mut tx = self
                .pool
                .begin()
                .await
                .context("failed to start task reorder transaction")?;

            for (id, position) in positions {
                sqlx::query("UPDATE tasks SET position = ?, updated_at = ? WHERE id = ?")
                    .bind(*position)
                    .bind(now_iso())
                    .bind(id.to_string())
                    .execute(&mut *tx)
                    .await
                    .with_context(|| format!("failed to update task position for {id}"))?;
            }

            tx.commit()
                .await
                .context("failed to commit task reorder transaction")?;
            Ok(())
        })
    }

    pub fn update_task_tmux(
        &self,
        id: Uuid,
        tmux_session_name: Option<String>,
        worktree_path: Option<String>,
    ) -> Result<()> {
        self.block_on(async {
            sqlx::query(
                "UPDATE tasks
                 SET tmux_session_name = ?,
                     worktree_path = ?,
                     updated_at = ?
                 WHERE id = ?",
            )
            .bind(tmux_session_name)
            .bind(worktree_path)
            .bind(now_iso())
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .context("failed to update task tmux metadata")?;
            Ok(())
        })
    }

    pub fn update_task_status(&self, id: Uuid, status: impl AsRef<str>) -> Result<()> {
        self.block_on(async {
            sqlx::query("UPDATE tasks SET tmux_status = ?, updated_at = ? WHERE id = ?")
                .bind(status.as_ref())
                .bind(now_iso())
                .bind(id.to_string())
                .execute(&self.pool)
                .await
                .context("failed to update task status")?;
            Ok(())
        })
    }

    pub fn update_task_status_metadata(
        &self,
        id: Uuid,
        status_source: impl AsRef<str>,
        status_fetched_at: Option<String>,
        status_error: Option<String>,
    ) -> Result<()> {
        self.block_on(async {
            sqlx::query(
                "UPDATE tasks
                 SET status_source = ?,
                     status_fetched_at = ?,
                     status_error = ?,
                     updated_at = ?
                 WHERE id = ?",
            )
            .bind(status_source.as_ref())
            .bind(status_fetched_at)
            .bind(status_error)
            .bind(now_iso())
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .context("failed to update task status metadata")?;
            Ok(())
        })
    }

    pub fn update_task_session_binding(
        &self,
        id: Uuid,
        opencode_session_id: Option<String>,
    ) -> Result<()> {
        self.block_on(async {
            sqlx::query(
                "UPDATE tasks
                 SET opencode_session_id = ?,
                     updated_at = ?
                 WHERE id = ?",
            )
            .bind(opencode_session_id)
            .bind(now_iso())
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .context("failed to update task opencode session binding")?;
            Ok(())
        })
    }

    pub fn update_task_todo(&self, id: Uuid, session_todo_json: Option<String>) -> Result<()> {
        self.block_on(async {
            sqlx::query(
                "UPDATE tasks
                 SET session_todo_json = ?,
                     updated_at = ?
                 WHERE id = ?",
            )
            .bind(session_todo_json)
            .bind(now_iso())
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .context("failed to update task todo metadata")?;
            Ok(())
        })
    }

    pub fn delete_task(&self, id: Uuid) -> Result<()> {
        self.block_on(async {
            sqlx::query("DELETE FROM tasks WHERE id = ?")
                .bind(id.to_string())
                .execute(&self.pool)
                .await
                .context("failed to delete task")?;
            Ok(())
        })
    }

    pub fn add_category(
        &self,
        name: impl AsRef<str>,
        position: i64,
        color: Option<String>,
    ) -> Result<Category> {
        let now = now_iso();
        let id = Uuid::new_v4();

        self.block_on(async {
            sqlx::query(
                "INSERT INTO categories (id, name, position, color, created_at) VALUES (?, ?, ?, ?, ?)",
            )
            .bind(id.to_string())
            .bind(name.as_ref())
            .bind(position)
            .bind(color)
            .bind(now)
            .execute(&self.pool)
            .await
            .context("failed to insert category")?;

            self.get_category_async(id).await
        })
    }

    pub fn list_categories(&self) -> Result<Vec<Category>> {
        self.block_on(async {
            let rows = sqlx::query(
                "SELECT id, name, position, color, created_at FROM categories ORDER BY position ASC",
            )
            .fetch_all(&self.pool)
            .await
            .context("failed to load categories")?;

            rows.into_iter()
                .map(map_category_row)
                .collect::<Result<Vec<_>>>()
        })
    }

    pub fn update_category_position(&self, id: Uuid, position: i64) -> Result<()> {
        self.block_on(async {
            sqlx::query("UPDATE categories SET position = ? WHERE id = ?")
                .bind(position)
                .bind(id.to_string())
                .execute(&self.pool)
                .await
                .context("failed to update category position")?;
            Ok(())
        })
    }

    pub fn rename_category(&self, id: Uuid, name: impl AsRef<str>) -> Result<()> {
        self.block_on(async {
            sqlx::query("UPDATE categories SET name = ? WHERE id = ?")
                .bind(name.as_ref())
                .bind(id.to_string())
                .execute(&self.pool)
                .await
                .context("failed to rename category")?;
            Ok(())
        })
    }

    pub fn update_category_color(&self, id: Uuid, color: Option<String>) -> Result<()> {
        self.block_on(async {
            sqlx::query("UPDATE categories SET color = ? WHERE id = ?")
                .bind(color)
                .bind(id.to_string())
                .execute(&self.pool)
                .await
                .context("failed to update category color")?;
            Ok(())
        })
    }

    pub fn delete_category(&self, id: Uuid) -> Result<()> {
        self.block_on(async {
            sqlx::query("DELETE FROM categories WHERE id = ?")
                .bind(id.to_string())
                .execute(&self.pool)
                .await
                .context("failed to delete category")?;
            Ok(())
        })
    }

    pub fn increment_command_usage(&self, command_id: &str) -> Result<()> {
        let now = now_iso();
        self.block_on(async {
            sqlx::query(
                "INSERT INTO command_frequency (command_id, use_count, last_used)
                 VALUES (?, 1, ?)
                 ON CONFLICT(command_id) DO UPDATE SET
                     use_count = use_count + 1,
                     last_used = ?",
            )
            .bind(command_id)
            .bind(now.clone())
            .bind(now)
            .execute(&self.pool)
            .await
            .context("failed to increment command usage")?;
            Ok(())
        })
    }

    pub fn get_command_frequencies(&self) -> Result<HashMap<String, CommandFrequency>> {
        self.block_on(async {
            let rows = sqlx::query(
                "SELECT command_id, use_count, last_used FROM command_frequency ORDER BY use_count DESC",
            )
            .fetch_all(&self.pool)
            .await
            .context("failed to load command frequencies")?;

            let mut map = HashMap::new();
            for row in rows {
                let freq = CommandFrequency {
                    command_id: row
                        .try_get::<String, _>("command_id")
                        .context("failed to decode command_frequency.command_id")?,
                    use_count: row
                        .try_get::<i64, _>("use_count")
                        .context("failed to decode command_frequency.use_count")?,
                    last_used: row
                        .try_get::<String, _>("last_used")
                        .context("failed to decode command_frequency.last_used")?,
                };
                map.insert(freq.command_id.clone(), freq);
            }
            Ok(map)
        })
    }

    fn run_migrations(&self) -> Result<()> {
        self.block_on(async {
            sqlx::migrate!("./migrations")
                .run(&self.pool)
                .await
                .context("failed to run sqlite migrations")?;

            self.migrate_tasks_status_columns().await?;
            self.migrate_categories_color_column().await?;

            sqlx::query("UPDATE tasks SET status_source = 'none' WHERE status_source IS NULL")
                .execute(&self.pool)
                .await
                .context("failed to backfill tasks.status_source")?;

            Ok(())
        })
    }

    async fn migrate_tasks_status_columns(&self) -> Result<()> {
        self.exec_optional_duplicate(
            "ALTER TABLE tasks ADD COLUMN status_source TEXT NOT NULL DEFAULT 'none'",
            "failed to migrate tasks.status_source",
        )
        .await?;
        self.exec_optional_duplicate(
            "ALTER TABLE tasks ADD COLUMN status_fetched_at TEXT",
            "failed to migrate tasks.status_fetched_at",
        )
        .await?;
        self.exec_optional_duplicate(
            "ALTER TABLE tasks ADD COLUMN status_error TEXT",
            "failed to migrate tasks.status_error",
        )
        .await?;
        self.exec_optional_duplicate(
            "ALTER TABLE tasks ADD COLUMN session_todo_json TEXT",
            "failed to migrate tasks.session_todo_json",
        )
        .await?;
        self.exec_optional_duplicate(
            "ALTER TABLE tasks ADD COLUMN opencode_session_id TEXT",
            "failed to migrate tasks.opencode_session_id",
        )
        .await?;
        Ok(())
    }

    async fn migrate_categories_color_column(&self) -> Result<()> {
        self.exec_optional_duplicate(
            "ALTER TABLE categories ADD COLUMN color TEXT",
            "failed to add categories.color column",
        )
        .await
    }

    async fn exec_optional_duplicate(&self, query: &str, context_msg: &str) -> Result<()> {
        let context_msg_owned = context_msg.to_string();
        match sqlx::query(query).execute(&self.pool).await {
            Ok(_) => Ok(()),
            Err(err) if is_duplicate_column_err(&err) => Ok(()),
            Err(err) => Err(err).context(context_msg_owned),
        }
    }

    fn seed_default_categories(&self) -> Result<()> {
        self.block_on(async {
            let category_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM categories")
                .fetch_one(&self.pool)
                .await
                .context("failed to count categories")?;

            if category_count == 0 {
                let now = now_iso();
                let defaults = [("TODO", 0_i64), ("IN PROGRESS", 1_i64), ("DONE", 2_i64)];

                for (name, position) in defaults {
                    sqlx::query(
                        "INSERT INTO categories (id, name, position, color, created_at) VALUES (?, ?, ?, ?, ?)",
                    )
                    .bind(Uuid::new_v4().to_string())
                    .bind(name)
                    .bind(position)
                    .bind(None::<String>)
                    .bind(now.clone())
                    .execute(&self.pool)
                    .await
                    .with_context(|| format!("failed to seed default category {name}"))?;
                }
            }

            Ok(())
        })
    }

    fn get_repo(&self, id: Uuid) -> Result<Repo> {
        self.block_on(self.get_repo_async(id))
    }

    async fn get_repo_async(&self, id: Uuid) -> Result<Repo> {
        let row = sqlx::query(
            "SELECT id, path, name, default_base, remote_url, created_at, updated_at
             FROM repos WHERE id = ?",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .with_context(|| format!("failed to fetch repo {id}"))?
        .ok_or_else(|| anyhow!("repo {id} not found"))?;

        map_repo_row(row)
    }

    fn get_category(&self, id: Uuid) -> Result<Category> {
        self.block_on(self.get_category_async(id))
    }

    async fn get_category_async(&self, id: Uuid) -> Result<Category> {
        let row = sqlx::query(
            "SELECT id, name, position, color, created_at FROM categories WHERE id = ?",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .with_context(|| format!("failed to fetch category {id}"))?
        .ok_or_else(|| anyhow!("category {id} not found"))?;

        map_category_row(row)
    }

    async fn get_task_async(&self, id: Uuid) -> Result<Task> {
        let row = sqlx::query(
            "SELECT id, title, repo_id, branch, category_id, position, tmux_session_name,
                    worktree_path, tmux_status, status_source,
                    status_fetched_at, status_error, opencode_session_id, session_todo_json,
                    created_at, updated_at
             FROM tasks WHERE id = ?",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .with_context(|| format!("failed to fetch task {id}"))?
        .ok_or_else(|| anyhow!("task {id} not found"))?;

        map_task_row(row)
    }

    fn block_on<T>(&self, fut: impl Future<Output = Result<T>>) -> Result<T> {
        self.runtime.block_on(fut)
    }
}

fn map_repo_row(row: SqliteRow) -> Result<Repo> {
    Ok(Repo {
        id: parse_uuid_column(
            row.try_get::<String, _>("id")
                .context("failed to decode repos.id")?,
            "repos.id",
        )?,
        path: row
            .try_get::<String, _>("path")
            .context("failed to decode repos.path")?,
        name: row
            .try_get::<String, _>("name")
            .context("failed to decode repos.name")?,
        default_base: row
            .try_get::<Option<String>, _>("default_base")
            .context("failed to decode repos.default_base")?,
        remote_url: row
            .try_get::<Option<String>, _>("remote_url")
            .context("failed to decode repos.remote_url")?,
        created_at: row
            .try_get::<String, _>("created_at")
            .context("failed to decode repos.created_at")?,
        updated_at: row
            .try_get::<String, _>("updated_at")
            .context("failed to decode repos.updated_at")?,
    })
}

fn map_category_row(row: SqliteRow) -> Result<Category> {
    Ok(Category {
        id: parse_uuid_column(
            row.try_get::<String, _>("id")
                .context("failed to decode categories.id")?,
            "categories.id",
        )?,
        name: row
            .try_get::<String, _>("name")
            .context("failed to decode categories.name")?,
        position: row
            .try_get::<i64, _>("position")
            .context("failed to decode categories.position")?,
        color: row
            .try_get::<Option<String>, _>("color")
            .context("failed to decode categories.color")?,
        created_at: row
            .try_get::<String, _>("created_at")
            .context("failed to decode categories.created_at")?,
    })
}

fn map_task_row(row: SqliteRow) -> Result<Task> {
    Ok(Task {
        id: parse_uuid_column(
            row.try_get::<String, _>("id")
                .context("failed to decode tasks.id")?,
            "tasks.id",
        )?,
        title: row
            .try_get::<String, _>("title")
            .context("failed to decode tasks.title")?,
        repo_id: parse_uuid_column(
            row.try_get::<String, _>("repo_id")
                .context("failed to decode tasks.repo_id")?,
            "tasks.repo_id",
        )?,
        branch: row
            .try_get::<String, _>("branch")
            .context("failed to decode tasks.branch")?,
        category_id: parse_uuid_column(
            row.try_get::<String, _>("category_id")
                .context("failed to decode tasks.category_id")?,
            "tasks.category_id",
        )?,
        position: row
            .try_get::<i64, _>("position")
            .context("failed to decode tasks.position")?,
        tmux_session_name: row
            .try_get::<Option<String>, _>("tmux_session_name")
            .context("failed to decode tasks.tmux_session_name")?,
        worktree_path: row
            .try_get::<Option<String>, _>("worktree_path")
            .context("failed to decode tasks.worktree_path")?,
        tmux_status: row
            .try_get::<String, _>("tmux_status")
            .context("failed to decode tasks.tmux_status")?,
        status_source: row
            .try_get::<String, _>("status_source")
            .context("failed to decode tasks.status_source")?,
        status_fetched_at: row
            .try_get::<Option<String>, _>("status_fetched_at")
            .context("failed to decode tasks.status_fetched_at")?,
        status_error: row
            .try_get::<Option<String>, _>("status_error")
            .context("failed to decode tasks.status_error")?,
        opencode_session_id: row
            .try_get::<Option<String>, _>("opencode_session_id")
            .context("failed to decode tasks.opencode_session_id")?,
        session_todo_json: row
            .try_get::<Option<String>, _>("session_todo_json")
            .context("failed to decode tasks.session_todo_json")?,
        created_at: row
            .try_get::<String, _>("created_at")
            .context("failed to decode tasks.created_at")?,
        updated_at: row
            .try_get::<String, _>("updated_at")
            .context("failed to decode tasks.updated_at")?,
    })
}

fn is_duplicate_column_err(err: &sqlx::Error) -> bool {
    matches!(err, sqlx::Error::Database(db_err) if db_err.message().contains("duplicate column name"))
}

fn parse_uuid_column(value: String, column: &str) -> Result<Uuid> {
    Uuid::parse_str(&value).with_context(|| format!("invalid UUID in column {column}: {value}"))
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
    use std::{
        path::{Path, PathBuf},
        process::Command,
    };

    use anyhow::Result;
    use sqlx::Row;
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
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
    fn test_migration_adds_categories_color_column_without_data_loss() -> Result<()> {
        let path = temp_path("migration-category-color").join("opencode-kanban.sqlite");
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let legacy_id = Uuid::new_v4();
        seed_legacy_categories_without_color(&path, legacy_id)?;

        {
            let db = Database::open(&path)?;

            let column_names = db.block_on(async {
                let rows = sqlx::query("PRAGMA table_info(categories)")
                    .fetch_all(&db.pool)
                    .await?;
                let mut names = Vec::new();
                for row in rows {
                    names.push(row.try_get::<String, _>("name")?);
                }
                Ok::<Vec<String>, anyhow::Error>(names)
            })?;
            assert!(column_names.iter().any(|name| name == "color"));

            let legacy_color = db.block_on(async {
                sqlx::query_scalar::<_, Option<String>>("SELECT color FROM categories WHERE id = ?")
                    .bind(legacy_id.to_string())
                    .fetch_one(&db.pool)
                    .await
                    .map_err(anyhow::Error::from)
            })?;
            assert_eq!(legacy_color, None);
        }

        let _db = Database::open(&path)?;

        if let Some(parent) = path.parent() {
            std::fs::remove_dir_all(parent)?;
        }

        Ok(())
    }

    #[test]
    fn test_legacy_db_upgrade_without_migration_table() -> Result<()> {
        let path = temp_path("legacy-baseline").join("opencode-kanban.sqlite");
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let legacy_repo_id = Uuid::new_v4();
        seed_legacy_db_without_sqlx_migrations(&path, legacy_repo_id)?;

        let db = Database::open(&path)?;
        let repos = db.list_repos()?;
        assert_eq!(repos.len(), 1);
        assert_eq!(repos[0].id, legacy_repo_id);

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
        assert_eq!(task.opencode_session_id, None);
        assert_eq!(task.session_todo_json, None);

        let fetched = db.get_task(task.id)?;
        assert_eq!(fetched.id, task.id);

        db.update_task_position(task.id, 5)?;
        db.update_task_category(task.id, done_category, 1)?;
        db.update_task_tmux(
            task.id,
            Some("ok-task-crud-feature-db-layer".to_string()),
            Some("/tmp/task-crud-feature-db-layer".to_string()),
        )?;
        db.update_task_status(task.id, "running")?;
        db.update_task_status_metadata(
            task.id,
            "tmux",
            Some("2026-02-15T12:34:56Z".to_string()),
            Some("transient timeout".to_string()),
        )?;
        db.update_task_session_binding(task.id, Some("sid-task-crud".to_string()))?;
        db.update_task_todo(
            task.id,
            Some("[{\"content\":\"Write docs\",\"completed\":false}]".to_string()),
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
            updated.opencode_session_id.as_deref(),
            Some("sid-task-crud")
        );
        assert_eq!(
            updated.session_todo_json.as_deref(),
            Some("[{\"content\":\"Write docs\",\"completed\":false}]")
        );
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

        let category = db.add_category("REVIEW", 3, None)?;
        db.rename_category(category.id, "QA")?;
        db.update_category_position(category.id, 4)?;

        let categories = db.list_categories()?;
        let qa = categories
            .into_iter()
            .find(|c| c.id == category.id)
            .expect("category should exist");
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

        let duplicate_category = db.add_category("TODO", 99, None);
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

        let repo_id = Uuid::new_v4();
        let category_id = Uuid::new_v4();
        let task_id = Uuid::new_v4();
        seed_legacy_tasks_without_status_metadata(&path, repo_id, category_id, task_id)?;

        let db = Database::open(&path)?;
        let migrated_task = db.get_task(task_id)?;
        assert_eq!(migrated_task.tmux_status, "running");
        assert_eq!(migrated_task.status_source, "none");
        assert_eq!(migrated_task.status_fetched_at, None);
        assert_eq!(migrated_task.status_error, None);
        assert_eq!(migrated_task.opencode_session_id, None);
        assert_eq!(migrated_task.session_todo_json, None);

        let (status_source_type, session_todo_type, opencode_session_id_type) =
            db.block_on(async {
                let status_source_type: String = sqlx::query_scalar(
                    "SELECT type FROM pragma_table_info('tasks') WHERE name = 'status_source'",
                )
                .fetch_one(&db.pool)
                .await?;
                let session_todo_type: String = sqlx::query_scalar(
                    "SELECT type FROM pragma_table_info('tasks') WHERE name = 'session_todo_json'",
                )
                .fetch_one(&db.pool)
                .await?;
                let opencode_session_id_type: String = sqlx::query_scalar(
                "SELECT type FROM pragma_table_info('tasks') WHERE name = 'opencode_session_id'",
            )
            .fetch_one(&db.pool)
            .await?;
                Ok::<(String, String, String), anyhow::Error>((
                    status_source_type,
                    session_todo_type,
                    opencode_session_id_type,
                ))
            })?;
        assert_eq!(status_source_type, "TEXT");
        assert_eq!(session_todo_type, "TEXT");
        assert_eq!(opencode_session_id_type, "TEXT");

        db.update_task_status(task_id, "dead")?;
        db.update_task_status_metadata(task_id, "server", None, None)?;
        db.update_task_session_binding(task_id, Some("sid-legacy".to_string()))?;
        let updated = db.get_task(task_id)?;
        assert_eq!(updated.tmux_status, "dead");
        assert_eq!(updated.status_source, "server");
        assert_eq!(updated.opencode_session_id.as_deref(), Some("sid-legacy"));

        if let Some(parent) = path.parent() {
            std::fs::remove_dir_all(parent)?;
        }
        Ok(())
    }

    #[test]
    fn test_increment_new_command() -> Result<()> {
        let db = Database::open(":memory:")?;

        db.increment_command_usage("create-worktree")?;

        let freqs = db.get_command_frequencies()?;
        assert_eq!(freqs.len(), 1);
        let freq = freqs
            .get("create-worktree")
            .expect("frequency should exist");
        assert_eq!(freq.use_count, 1);

        Ok(())
    }

    #[test]
    fn test_increment_existing_command() -> Result<()> {
        let db = Database::open(":memory:")?;

        db.increment_command_usage("create-worktree")?;
        db.increment_command_usage("create-worktree")?;
        db.increment_command_usage("create-worktree")?;

        let freqs = db.get_command_frequencies()?;
        let freq = freqs
            .get("create-worktree")
            .expect("frequency should exist");
        assert_eq!(freq.use_count, 3);

        Ok(())
    }

    #[test]
    fn test_get_frequencies_empty() -> Result<()> {
        let db = Database::open(":memory:")?;

        let freqs = db.get_command_frequencies()?;
        assert!(freqs.is_empty());

        Ok(())
    }

    #[test]
    fn test_get_frequencies_with_data() -> Result<()> {
        let db = Database::open(":memory:")?;

        db.increment_command_usage("cmd-a")?;
        db.increment_command_usage("cmd-a")?;
        db.increment_command_usage("cmd-b")?;

        let freqs = db.get_command_frequencies()?;
        assert_eq!(freqs.len(), 2);

        let cmd_a = freqs.get("cmd-a").expect("cmd-a should exist");
        let cmd_b = freqs.get("cmd-b").expect("cmd-b should exist");
        assert_eq!(cmd_a.use_count, 2);
        assert_eq!(cmd_b.use_count, 1);

        Ok(())
    }

    fn seed_legacy_categories_without_color(path: &Path, legacy_id: Uuid) -> Result<()> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        rt.block_on(async {
            let options = SqliteConnectOptions::new()
                .filename(path)
                .create_if_missing(true);
            let pool = SqlitePoolOptions::new()
                .max_connections(1)
                .connect_with(options)
                .await?;

            sqlx::query(
                "CREATE TABLE categories (
                    id TEXT PRIMARY KEY,
                    name TEXT NOT NULL UNIQUE,
                    position INTEGER NOT NULL,
                    created_at TEXT NOT NULL
                 )",
            )
            .execute(&pool)
            .await?;

            sqlx::query(
                "INSERT INTO categories (id, name, position, created_at) VALUES (?, ?, ?, ?)",
            )
            .bind(legacy_id.to_string())
            .bind("LEGACY")
            .bind(0_i64)
            .bind("2026-01-01T00:00:00Z")
            .execute(&pool)
            .await?;

            Ok::<(), anyhow::Error>(())
        })
    }

    fn seed_legacy_db_without_sqlx_migrations(path: &Path, repo_id: Uuid) -> Result<()> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        rt.block_on(async {
            let options = SqliteConnectOptions::new().filename(path).create_if_missing(true);
            let pool = SqlitePoolOptions::new()
                .max_connections(1)
                .connect_with(options)
                .await?;

            sqlx::query(
                "CREATE TABLE repos (
                    id TEXT PRIMARY KEY,
                    path TEXT NOT NULL UNIQUE,
                    name TEXT NOT NULL,
                    default_base TEXT,
                    remote_url TEXT,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                 )",
            )
            .execute(&pool)
            .await?;

            sqlx::query(
                "INSERT INTO repos (id, path, name, default_base, remote_url, created_at, updated_at)
                 VALUES (?, ?, ?, NULL, NULL, ?, ?)",
            )
            .bind(repo_id.to_string())
            .bind("/tmp/legacy-repo")
            .bind("legacy-repo")
            .bind("2026-02-15T00:00:00Z")
            .bind("2026-02-15T00:00:00Z")
            .execute(&pool)
            .await?;

            Ok::<(), anyhow::Error>(())
        })
    }

    fn seed_legacy_tasks_without_status_metadata(
        path: &Path,
        repo_id: Uuid,
        category_id: Uuid,
        task_id: Uuid,
    ) -> Result<()> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        rt.block_on(async {
            let options = SqliteConnectOptions::new().filename(path).create_if_missing(true);
            let pool = SqlitePoolOptions::new()
                .max_connections(1)
                .connect_with(options)
                .await?;

            sqlx::query(
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
                    worktree_path TEXT,
                    tmux_status TEXT DEFAULT 'unknown',
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL,
                    UNIQUE(repo_id, branch)
                 );",
            )
            .execute(&pool)
            .await?;

            sqlx::query(
                "INSERT INTO repos (id, path, name, default_base, remote_url, created_at, updated_at)
                 VALUES (?, ?, ?, NULL, NULL, ?, ?)",
            )
            .bind(repo_id.to_string())
            .bind("/tmp/legacy-repo")
            .bind("legacy-repo")
            .bind("2026-02-15T00:00:00Z")
            .bind("2026-02-15T00:00:00Z")
            .execute(&pool)
            .await?;
            sqlx::query(
                "INSERT INTO categories (id, name, position, created_at) VALUES (?, ?, 0, ?)",
            )
            .bind(category_id.to_string())
            .bind("TODO")
            .bind("2026-02-15T00:00:00Z")
            .execute(&pool)
            .await?;
            sqlx::query(
                "INSERT INTO tasks (
                    id, title, repo_id, branch, category_id, position, tmux_session_name,
                    worktree_path, tmux_status, created_at, updated_at
                 ) VALUES (?, ?, ?, ?, ?, 0, NULL, NULL, ?, ?, ?)",
            )
            .bind(task_id.to_string())
            .bind("legacy task")
            .bind(repo_id.to_string())
            .bind("feature/legacy")
            .bind(category_id.to_string())
            .bind("running")
            .bind("2026-02-15T00:00:00Z")
            .bind("2026-02-15T00:00:00Z")
            .execute(&pool)
            .await?;

            Ok::<(), anyhow::Error>(())
        })
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
