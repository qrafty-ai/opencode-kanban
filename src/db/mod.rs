#![allow(dead_code)]

use std::{
    collections::{HashMap, HashSet},
    fs,
    future::Future,
    path::{Path, PathBuf},
    process::Command,
    str::FromStr,
    sync::OnceLock,
};

use anyhow::{Context, Result, anyhow, bail};
use chrono::Utc;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteRow};
use sqlx::{Row, SqlitePool};
use tokio::runtime::{Builder as RuntimeBuilder, Handle, RuntimeFlavor};
use uuid::Uuid;

use crate::types::{Category, CommandFrequency, Repo, Task};

const DEFAULT_TMUX_STATUS: &str = "unknown";
const DEFAULT_STATUS_SOURCE: &str = "none";

#[derive(Clone)]
pub struct Database {
    pool: SqlitePool,
}

impl Database {
    pub async fn open_async(path: impl AsRef<Path>) -> Result<Self> {
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

        let connect_options = sqlite_connect_options(path_ref)?;
        let max_connections = if path_ref == Path::new(":memory:") {
            1
        } else {
            5
        };

        let pool = SqlitePoolOptions::new()
            .max_connections(max_connections)
            .connect_with(connect_options)
            .await
            .with_context(|| format!("failed to open sqlite db at {}", path_ref.display()))?;

        let db = Self { pool };
        db.run_migrations_async().await?;
        db.seed_default_categories_async().await?;
        Ok(db)
    }

    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        block_on_db(Self::open_async(path))
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    pub async fn add_repo_async(&self, path: PathBuf) -> Result<Repo> {
        let path_buf = fs::canonicalize(&path)
            .with_context(|| format!("failed to canonicalize repo path {}", path.display()))?;
        let path_str = path_buf
            .to_str()
            .ok_or_else(|| anyhow!("repo path is not valid UTF-8: {}", path_buf.display()))?
            .to_string();
        let name = derive_repo_name(&path_buf);
        let default_base = detect_default_base(&path_buf);
        let remote_url = detect_remote_url(&path_buf);
        let now = now_iso();
        let id = Uuid::new_v4();

        sqlx::query(
            "INSERT INTO repos (id, path, name, default_base, remote_url, created_at, updated_at)
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
    }

    pub fn add_repo(&self, path: impl AsRef<Path>) -> Result<Repo> {
        block_on_db(self.add_repo_async(path.as_ref().to_path_buf()))
    }

    pub async fn list_repos_async(&self) -> Result<Vec<Repo>> {
        let rows = sqlx::query(
            "SELECT id, path, name, default_base, remote_url, created_at, updated_at
             FROM repos ORDER BY created_at ASC",
        )
        .fetch_all(&self.pool)
        .await
        .context("failed to load repos")?;

        rows.into_iter().map(|row| map_repo_row(&row)).collect()
    }

    pub fn list_repos(&self) -> Result<Vec<Repo>> {
        block_on_db(self.list_repos_async())
    }

    pub async fn update_repo_name_async(&self, id: Uuid, new_name: &str) -> Result<()> {
        let now = now_iso();
        sqlx::query("UPDATE repos SET name = ?, updated_at = ? WHERE id = ?")
            .bind(new_name)
            .bind(&now)
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .context("failed to update repo name")?;
        Ok(())
    }

    pub fn update_repo_name(&self, id: Uuid, new_name: &str) -> Result<()> {
        block_on_db(self.update_repo_name_async(id, new_name))
    }

    pub async fn delete_repo_async(&self, id: Uuid) -> Result<()> {
        let task_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM tasks WHERE repo_id = ?")
            .bind(id.to_string())
            .fetch_one(&self.pool)
            .await
            .context("failed to count tasks for repo")?;

        if task_count > 0 {
            anyhow::bail!(
                "cannot delete repo: {} task(s) still reference it",
                task_count
            );
        }

        sqlx::query("DELETE FROM repos WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .context("failed to delete repo")?;

        Ok(())
    }

    pub fn delete_repo(&self, id: Uuid) -> Result<()> {
        block_on_db(self.delete_repo_async(id))
    }

    pub async fn add_task_async(
        &self,
        repo_id: Uuid,
        branch: String,
        title: String,
        category_id: Uuid,
    ) -> Result<Task> {
        if branch.trim().is_empty() {
            bail!("branch cannot be empty");
        }

        let position: i64 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(position) + 1, 0) FROM tasks WHERE category_id = ?",
        )
        .bind(category_id.to_string())
        .fetch_one(&self.pool)
        .await?;

        let resolved_title = if title.trim().is_empty() {
            branch.clone()
        } else {
            title
        };

        let now = now_iso();
        let id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO tasks (
                id, title, repo_id, branch, category_id, position, tmux_session_name,
                worktree_path, tmux_status, status_source,
                status_fetched_at, status_error, opencode_session_id,
                attach_overlay_shown, archived, archived_at, created_at, updated_at
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id.to_string())
        .bind(resolved_title)
        .bind(repo_id.to_string())
        .bind(branch)
        .bind(category_id.to_string())
        .bind(position)
        .bind(Option::<String>::None)
        .bind(Option::<String>::None)
        .bind(DEFAULT_TMUX_STATUS)
        .bind(DEFAULT_STATUS_SOURCE)
        .bind(Option::<String>::None)
        .bind(Option::<String>::None)
        .bind(Option::<String>::None)
        .bind(0)
        .bind(0)
        .bind(Option::<String>::None)
        .bind(now.clone())
        .bind(now)
        .execute(&self.pool)
        .await
        .context("failed to insert task")?;

        self.get_task_async(id).await
    }

    pub fn add_task(
        &self,
        repo_id: Uuid,
        branch: impl AsRef<str>,
        title: impl AsRef<str>,
        category_id: Uuid,
    ) -> Result<Task> {
        block_on_db(self.add_task_async(
            repo_id,
            branch.as_ref().to_string(),
            title.as_ref().to_string(),
            category_id,
        ))
    }

    pub async fn get_task_async(&self, id: Uuid) -> Result<Task> {
        let row = sqlx::query(
            "SELECT id, title, repo_id, branch, category_id, position, tmux_session_name,
                    worktree_path, tmux_status, status_source,
                    status_fetched_at, status_error, opencode_session_id,
                    attach_overlay_shown, archived, archived_at,
                    created_at, updated_at
             FROM tasks WHERE id = ?",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await?;

        let row = row.with_context(|| format!("task {id} not found"))?;
        map_task_row(&row)
    }

    pub fn get_task(&self, id: Uuid) -> Result<Task> {
        block_on_db(self.get_task_async(id))
    }

    pub async fn list_tasks_async(&self) -> Result<Vec<Task>> {
        let rows = sqlx::query(
            "SELECT id, title, repo_id, branch, category_id, position, tmux_session_name,
                    worktree_path, tmux_status, status_source,
                    status_fetched_at, status_error, opencode_session_id,
                    attach_overlay_shown, archived, archived_at,
                    created_at, updated_at
             FROM tasks WHERE archived = 0
             ORDER BY category_id ASC, position ASC, created_at ASC",
        )
        .fetch_all(&self.pool)
        .await
        .context("failed to load tasks")?;

        rows.into_iter().map(|row| map_task_row(&row)).collect()
    }

    pub fn list_tasks(&self) -> Result<Vec<Task>> {
        block_on_db(self.list_tasks_async())
    }

    pub async fn list_archived_tasks_async(&self) -> Result<Vec<Task>> {
        let rows = sqlx::query(
            "SELECT id, title, repo_id, branch, category_id, position, tmux_session_name,
                    worktree_path, tmux_status, status_source,
                    status_fetched_at, status_error, opencode_session_id,
                    attach_overlay_shown, archived, archived_at,
                    created_at, updated_at
             FROM tasks WHERE archived = 1
             ORDER BY archived_at DESC, updated_at DESC",
        )
        .fetch_all(&self.pool)
        .await
        .context("failed to load archived tasks")?;

        rows.into_iter().map(|row| map_task_row(&row)).collect()
    }

    pub fn list_archived_tasks(&self) -> Result<Vec<Task>> {
        block_on_db(self.list_archived_tasks_async())
    }

    pub async fn archive_task_async(&self, id: Uuid) -> Result<()> {
        let now = now_iso();
        sqlx::query(
            "UPDATE tasks
             SET archived = 1,
                 archived_at = ?,
                 updated_at = ?
             WHERE id = ?",
        )
        .bind(now.clone())
        .bind(now)
        .bind(id.to_string())
        .execute(&self.pool)
        .await
        .context("failed to archive task")?;
        Ok(())
    }

    pub fn archive_task(&self, id: Uuid) -> Result<()> {
        block_on_db(self.archive_task_async(id))
    }

    pub async fn unarchive_task_async(&self, id: Uuid) -> Result<()> {
        sqlx::query(
            "UPDATE tasks
             SET archived = 0,
                 archived_at = NULL,
                 updated_at = ?
             WHERE id = ?",
        )
        .bind(now_iso())
        .bind(id.to_string())
        .execute(&self.pool)
        .await
        .context("failed to unarchive task")?;
        Ok(())
    }

    pub fn unarchive_task(&self, id: Uuid) -> Result<()> {
        block_on_db(self.unarchive_task_async(id))
    }

    pub async fn update_task_category_async(
        &self,
        id: Uuid,
        category_id: Uuid,
        position: i64,
    ) -> Result<()> {
        sqlx::query("UPDATE tasks SET category_id = ?, position = ?, updated_at = ? WHERE id = ?")
            .bind(category_id.to_string())
            .bind(position)
            .bind(now_iso())
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .context("failed to update task category")?;
        Ok(())
    }

    pub fn update_task_category(&self, id: Uuid, category_id: Uuid, position: i64) -> Result<()> {
        block_on_db(self.update_task_category_async(id, category_id, position))
    }

    pub async fn update_task_position_async(&self, id: Uuid, position: i64) -> Result<()> {
        sqlx::query("UPDATE tasks SET position = ?, updated_at = ? WHERE id = ?")
            .bind(position)
            .bind(now_iso())
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .context("failed to update task position")?;
        Ok(())
    }

    pub fn update_task_position(&self, id: Uuid, position: i64) -> Result<()> {
        block_on_db(self.update_task_position_async(id, position))
    }

    pub async fn update_task_title_async(&self, id: Uuid, title: impl AsRef<str>) -> Result<()> {
        sqlx::query("UPDATE tasks SET title = ?, updated_at = ? WHERE id = ?")
            .bind(title.as_ref())
            .bind(now_iso())
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .context("failed to update task title")?;
        Ok(())
    }

    pub fn update_task_title(&self, id: Uuid, title: impl AsRef<str>) -> Result<()> {
        block_on_db(self.update_task_title_async(id, title))
    }

    pub async fn update_task_tmux_async(
        &self,
        id: Uuid,
        tmux_session_name: Option<String>,
        worktree_path: Option<String>,
    ) -> Result<()> {
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
    }

    pub fn update_task_tmux(
        &self,
        id: Uuid,
        tmux_session_name: Option<String>,
        worktree_path: Option<String>,
    ) -> Result<()> {
        block_on_db(self.update_task_tmux_async(id, tmux_session_name, worktree_path))
    }

    pub async fn update_task_status_async(&self, id: Uuid, status: impl AsRef<str>) -> Result<()> {
        sqlx::query("UPDATE tasks SET tmux_status = ?, updated_at = ? WHERE id = ?")
            .bind(status.as_ref())
            .bind(now_iso())
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .context("failed to update task status")?;
        Ok(())
    }

    pub fn update_task_status(&self, id: Uuid, status: impl AsRef<str>) -> Result<()> {
        block_on_db(self.update_task_status_async(id, status))
    }

    pub async fn update_task_status_metadata_async(
        &self,
        id: Uuid,
        status_source: impl AsRef<str>,
        status_fetched_at: Option<String>,
        status_error: Option<String>,
    ) -> Result<()> {
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
    }

    pub fn update_task_status_metadata(
        &self,
        id: Uuid,
        status_source: impl AsRef<str>,
        status_fetched_at: Option<String>,
        status_error: Option<String>,
    ) -> Result<()> {
        block_on_db(self.update_task_status_metadata_async(
            id,
            status_source,
            status_fetched_at,
            status_error,
        ))
    }

    pub async fn update_task_session_binding_async(
        &self,
        id: Uuid,
        opencode_session_id: Option<String>,
    ) -> Result<()> {
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
    }

    pub fn update_task_session_binding(
        &self,
        id: Uuid,
        opencode_session_id: Option<String>,
    ) -> Result<()> {
        block_on_db(self.update_task_session_binding_async(id, opencode_session_id))
    }

    pub async fn update_task_attach_overlay_shown_async(
        &self,
        id: Uuid,
        attach_overlay_shown: bool,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE tasks
             SET attach_overlay_shown = ?,
                 updated_at = ?
             WHERE id = ?",
        )
        .bind(if attach_overlay_shown { 1 } else { 0 })
        .bind(now_iso())
        .bind(id.to_string())
        .execute(&self.pool)
        .await
        .context("failed to update task attach overlay state")?;
        Ok(())
    }

    pub fn update_task_attach_overlay_shown(
        &self,
        id: Uuid,
        attach_overlay_shown: bool,
    ) -> Result<()> {
        block_on_db(self.update_task_attach_overlay_shown_async(id, attach_overlay_shown))
    }

    pub async fn delete_task_async(&self, id: Uuid) -> Result<()> {
        sqlx::query("DELETE FROM tasks WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .context("failed to delete task")?;
        Ok(())
    }

    pub fn delete_task(&self, id: Uuid) -> Result<()> {
        block_on_db(self.delete_task_async(id))
    }

    pub async fn add_category_async(
        &self,
        name: impl AsRef<str>,
        position: i64,
        color: Option<String>,
    ) -> Result<Category> {
        self.add_category_with_slug_async(name, None, position, color)
            .await
    }

    pub async fn add_category_with_slug_async(
        &self,
        name: impl AsRef<str>,
        slug: Option<&str>,
        position: i64,
        color: Option<String>,
    ) -> Result<Category> {
        let normalized_slug = normalize_category_slug(slug.unwrap_or_else(|| name.as_ref()));
        let now = now_iso();
        let id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO categories (id, slug, name, position, color, created_at) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(id.to_string())
        .bind(normalized_slug)
        .bind(name.as_ref())
        .bind(position)
        .bind(color)
        .bind(now)
        .execute(&self.pool)
        .await
        .context("failed to insert category")?;

        self.get_category_async(id).await
    }

    pub fn add_category(
        &self,
        name: impl AsRef<str>,
        position: i64,
        color: Option<String>,
    ) -> Result<Category> {
        block_on_db(self.add_category_async(name, position, color))
    }

    pub fn add_category_with_slug(
        &self,
        name: impl AsRef<str>,
        slug: Option<&str>,
        position: i64,
        color: Option<String>,
    ) -> Result<Category> {
        block_on_db(self.add_category_with_slug_async(name, slug, position, color))
    }

    pub async fn list_categories_async(&self) -> Result<Vec<Category>> {
        let rows = sqlx::query(
            "SELECT id, slug, name, position, color, created_at FROM categories ORDER BY position ASC",
        )
        .fetch_all(&self.pool)
        .await
        .context("failed to load categories")?;

        rows.into_iter().map(|row| map_category_row(&row)).collect()
    }

    pub fn list_categories(&self) -> Result<Vec<Category>> {
        block_on_db(self.list_categories_async())
    }

    pub async fn update_category_position_async(&self, id: Uuid, position: i64) -> Result<()> {
        sqlx::query("UPDATE categories SET position = ? WHERE id = ?")
            .bind(position)
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .context("failed to update category position")?;
        Ok(())
    }

    pub fn update_category_position(&self, id: Uuid, position: i64) -> Result<()> {
        block_on_db(self.update_category_position_async(id, position))
    }

    pub async fn rename_category_async(&self, id: Uuid, name: impl AsRef<str>) -> Result<()> {
        let name = name.as_ref();
        sqlx::query("UPDATE categories SET name = ?, slug = ? WHERE id = ?")
            .bind(name)
            .bind(normalize_category_slug(name))
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .context("failed to rename category")?;
        Ok(())
    }

    pub fn rename_category(&self, id: Uuid, name: impl AsRef<str>) -> Result<()> {
        block_on_db(self.rename_category_async(id, name))
    }

    pub async fn update_category_slug_async(&self, id: Uuid, slug: impl AsRef<str>) -> Result<()> {
        sqlx::query("UPDATE categories SET slug = ? WHERE id = ?")
            .bind(normalize_category_slug(slug.as_ref()))
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .context("failed to update category slug")?;
        Ok(())
    }

    pub fn update_category_slug(&self, id: Uuid, slug: impl AsRef<str>) -> Result<()> {
        block_on_db(self.update_category_slug_async(id, slug))
    }

    pub async fn get_category_by_slug_async(
        &self,
        slug: impl AsRef<str>,
    ) -> Result<Option<Category>> {
        let row = sqlx::query(
            "SELECT id, slug, name, position, color, created_at FROM categories WHERE slug = ?",
        )
        .bind(normalize_category_slug(slug.as_ref()))
        .fetch_optional(&self.pool)
        .await?;

        row.map(|value| map_category_row(&value)).transpose()
    }

    pub fn get_category_by_slug(&self, slug: impl AsRef<str>) -> Result<Option<Category>> {
        block_on_db(self.get_category_by_slug_async(slug))
    }

    pub async fn update_category_color_async(&self, id: Uuid, color: Option<String>) -> Result<()> {
        sqlx::query("UPDATE categories SET color = ? WHERE id = ?")
            .bind(color)
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .context("failed to update category color")?;
        Ok(())
    }

    pub fn update_category_color(&self, id: Uuid, color: Option<String>) -> Result<()> {
        block_on_db(self.update_category_color_async(id, color))
    }

    pub async fn delete_category_async(&self, id: Uuid) -> Result<()> {
        sqlx::query("DELETE FROM categories WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .context("failed to delete category")?;
        Ok(())
    }

    pub fn delete_category(&self, id: Uuid) -> Result<()> {
        block_on_db(self.delete_category_async(id))
    }

    pub async fn increment_command_usage_async(&self, command_id: &str) -> Result<()> {
        let now = now_iso();
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
    }

    pub fn increment_command_usage(&self, command_id: &str) -> Result<()> {
        block_on_db(self.increment_command_usage_async(command_id))
    }

    pub async fn get_command_frequencies_async(&self) -> Result<HashMap<String, CommandFrequency>> {
        let rows = sqlx::query(
            "SELECT command_id, use_count, last_used FROM command_frequency ORDER BY use_count DESC",
        )
        .fetch_all(&self.pool)
        .await
        .context("failed to load command frequencies")?;

        let mut map = HashMap::new();
        for row in rows {
            let freq = CommandFrequency {
                command_id: row.try_get("command_id")?,
                use_count: row.try_get("use_count")?,
                last_used: row.try_get("last_used")?,
            };
            map.insert(freq.command_id.clone(), freq);
        }
        Ok(map)
    }

    pub fn get_command_frequencies(&self) -> Result<HashMap<String, CommandFrequency>> {
        block_on_db(self.get_command_frequencies_async())
    }

    async fn run_migrations_async(&self) -> Result<()> {
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS repos (
                id TEXT PRIMARY KEY,
                path TEXT NOT NULL UNIQUE,
                name TEXT NOT NULL,
                default_base TEXT,
                remote_url TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            )",
        )
        .execute(&self.pool)
        .await
        .context("failed to create repos table")?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS categories (
                id TEXT PRIMARY KEY,
                slug TEXT NOT NULL UNIQUE,
                name TEXT NOT NULL UNIQUE,
                position INTEGER NOT NULL,
                color TEXT,
                created_at TEXT NOT NULL
            )",
        )
        .execute(&self.pool)
        .await
        .context("failed to create categories table")?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS tasks (
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
                attach_overlay_shown INTEGER NOT NULL DEFAULT 0,
                archived INTEGER NOT NULL DEFAULT 0,
                archived_at TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                UNIQUE(repo_id, branch)
            )",
        )
        .execute(&self.pool)
        .await
        .context("failed to create tasks table")?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS command_frequency (
                command_id TEXT PRIMARY KEY,
                use_count INTEGER NOT NULL DEFAULT 0,
                last_used TEXT NOT NULL
            )",
        )
        .execute(&self.pool)
        .await
        .context("failed to create command_frequency table")?;

        execute_add_column_if_missing(
            &self.pool,
            "ALTER TABLE tasks ADD COLUMN status_source TEXT NOT NULL DEFAULT 'none'",
            "failed to migrate tasks.status_source",
        )
        .await?;
        execute_add_column_if_missing(
            &self.pool,
            "ALTER TABLE tasks ADD COLUMN status_fetched_at TEXT",
            "failed to migrate tasks.status_fetched_at",
        )
        .await?;
        execute_add_column_if_missing(
            &self.pool,
            "ALTER TABLE tasks ADD COLUMN status_error TEXT",
            "failed to migrate tasks.status_error",
        )
        .await?;
        execute_add_column_if_missing(
            &self.pool,
            "ALTER TABLE tasks ADD COLUMN opencode_session_id TEXT",
            "failed to migrate tasks.opencode_session_id",
        )
        .await?;
        execute_add_column_if_missing(
            &self.pool,
            "ALTER TABLE tasks ADD COLUMN attach_overlay_shown INTEGER NOT NULL DEFAULT 0",
            "failed to migrate tasks.attach_overlay_shown",
        )
        .await?;
        execute_add_column_if_missing(
            &self.pool,
            "ALTER TABLE tasks ADD COLUMN archived INTEGER NOT NULL DEFAULT 0",
            "failed to migrate tasks.archived",
        )
        .await?;
        execute_add_column_if_missing(
            &self.pool,
            "ALTER TABLE tasks ADD COLUMN archived_at TEXT",
            "failed to migrate tasks.archived_at",
        )
        .await?;

        sqlx::query("UPDATE tasks SET status_source = 'none' WHERE status_source IS NULL")
            .execute(&self.pool)
            .await
            .context("failed to backfill tasks.status_source")?;
        sqlx::query("UPDATE tasks SET attach_overlay_shown = 0 WHERE attach_overlay_shown IS NULL")
            .execute(&self.pool)
            .await
            .context("failed to backfill tasks.attach_overlay_shown")?;
        sqlx::query("UPDATE tasks SET archived = 0 WHERE archived IS NULL")
            .execute(&self.pool)
            .await
            .context("failed to backfill tasks.archived")?;

        self.migrate_categories_slug_column_async().await?;
        self.migrate_categories_color_column_async().await?;
        Ok(())
    }

    async fn migrate_categories_slug_column_async(&self) -> Result<()> {
        let rows = sqlx::query("PRAGMA table_info(categories)")
            .fetch_all(&self.pool)
            .await
            .context("failed to query categories table_info pragma")?;

        let has_slug_column = rows.into_iter().any(|row| {
            row.try_get::<String, _>(1)
                .map(|name| name == "slug")
                .unwrap_or(false)
        });

        if !has_slug_column {
            sqlx::query("ALTER TABLE categories ADD COLUMN slug TEXT")
                .execute(&self.pool)
                .await
                .context("failed to add categories.slug column")?;
        }

        let rows = sqlx::query(
            "SELECT id, name, slug FROM categories ORDER BY position ASC, created_at ASC, id ASC",
        )
        .fetch_all(&self.pool)
        .await
        .context("failed to load categories for slug migration")?;

        let mut used_slugs = HashSet::new();
        for row in rows {
            let id: String = row.try_get("id")?;
            let name: String = row.try_get("name")?;
            let existing_slug: Option<String> = row.try_get("slug")?;
            let base_slug = match existing_slug {
                Some(value) if !value.trim().is_empty() => normalize_category_slug(&value),
                _ => normalize_category_slug(&name),
            };
            let next_slug = next_available_slug(&base_slug, &used_slugs);
            used_slugs.insert(next_slug.clone());

            sqlx::query("UPDATE categories SET slug = ? WHERE id = ?")
                .bind(next_slug)
                .bind(id)
                .execute(&self.pool)
                .await
                .context("failed to backfill categories.slug")?;
        }

        sqlx::query("CREATE UNIQUE INDEX IF NOT EXISTS idx_categories_slug ON categories(slug)")
            .execute(&self.pool)
            .await
            .context("failed to create categories.slug unique index")?;

        Ok(())
    }

    async fn migrate_categories_color_column_async(&self) -> Result<()> {
        let rows = sqlx::query("PRAGMA table_info(categories)")
            .fetch_all(&self.pool)
            .await
            .context("failed to query categories table_info pragma")?;

        let has_color_column = rows.into_iter().any(|row| {
            row.try_get::<String, _>(1)
                .map(|name| name == "color")
                .unwrap_or(false)
        });

        if !has_color_column {
            sqlx::query("ALTER TABLE categories ADD COLUMN color TEXT")
                .execute(&self.pool)
                .await
                .context("failed to add categories.color column")?;
        }

        Ok(())
    }

    async fn seed_default_categories_async(&self) -> Result<()> {
        let category_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM categories")
            .fetch_one(&self.pool)
            .await
            .context("failed to count categories")?;

        if category_count == 0 {
            self.add_category_async("TODO", 0, None).await?;
            self.add_category_async("IN PROGRESS", 1, None).await?;
            self.add_category_async("DONE", 2, None).await?;
        }

        Ok(())
    }

    async fn get_repo_async(&self, id: Uuid) -> Result<Repo> {
        let row = sqlx::query(
            "SELECT id, path, name, default_base, remote_url, created_at, updated_at
             FROM repos WHERE id = ?",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await?;

        let row = row.with_context(|| format!("repo {id} not found"))?;
        map_repo_row(&row)
    }

    async fn get_category_async(&self, id: Uuid) -> Result<Category> {
        let row = sqlx::query(
            "SELECT id, slug, name, position, color, created_at FROM categories WHERE id = ?",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await?;

        let row = row.with_context(|| format!("category {id} not found"))?;
        map_category_row(&row)
    }
}

fn sqlite_connect_options(path_ref: &Path) -> Result<SqliteConnectOptions> {
    if path_ref == Path::new(":memory:") {
        return SqliteConnectOptions::from_str("sqlite::memory:")
            .map(|options| options.foreign_keys(true))
            .context("failed to build in-memory sqlite connect options");
    }

    Ok(SqliteConnectOptions::new()
        .filename(path_ref)
        .create_if_missing(true)
        .foreign_keys(true)
        .journal_mode(SqliteJournalMode::Wal))
}

fn block_on_db<F, T>(future: F) -> Result<T>
where
    F: Future<Output = Result<T>>,
{
    match Handle::try_current() {
        Ok(handle) => match handle.runtime_flavor() {
            RuntimeFlavor::MultiThread => tokio::task::block_in_place(|| handle.block_on(future)),
            RuntimeFlavor::CurrentThread => global_db_runtime()?.block_on(future),
            _ => handle.block_on(future),
        },
        Err(_) => global_db_runtime()?.block_on(future),
    }
}

fn global_db_runtime() -> Result<&'static tokio::runtime::Runtime> {
    static RUNTIME: OnceLock<Result<tokio::runtime::Runtime, String>> = OnceLock::new();
    let result = RUNTIME.get_or_init(|| {
        RuntimeBuilder::new_multi_thread()
            .enable_all()
            .build()
            .map_err(|err| err.to_string())
    });

    result
        .as_ref()
        .map_err(|err| anyhow::anyhow!("failed to initialize global DB runtime: {err}"))
}

async fn execute_add_column_if_missing(pool: &SqlitePool, sql: &str, context: &str) -> Result<()> {
    match sqlx::query(sql).execute(pool).await {
        Ok(_) => Ok(()),
        Err(err) if is_duplicate_column_err(&err) => Ok(()),
        Err(err) => Err(err).context(context.to_string()),
    }
}

fn map_repo_row(row: &SqliteRow) -> Result<Repo> {
    Ok(Repo {
        id: parse_uuid_column(row.try_get::<String, _>("id")?)?,
        path: row.try_get("path")?,
        name: row.try_get("name")?,
        default_base: row.try_get("default_base")?,
        remote_url: row.try_get("remote_url")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn map_category_row(row: &SqliteRow) -> Result<Category> {
    Ok(Category {
        id: parse_uuid_column(row.try_get::<String, _>("id")?)?,
        slug: row.try_get("slug")?,
        name: row.try_get("name")?,
        position: row.try_get("position")?,
        color: row.try_get("color")?,
        created_at: row.try_get("created_at")?,
    })
}

fn map_task_row(row: &SqliteRow) -> Result<Task> {
    Ok(Task {
        id: parse_uuid_column(row.try_get::<String, _>("id")?)?,
        title: row.try_get("title")?,
        repo_id: parse_uuid_column(row.try_get::<String, _>("repo_id")?)?,
        branch: row.try_get("branch")?,
        category_id: parse_uuid_column(row.try_get::<String, _>("category_id")?)?,
        position: row.try_get("position")?,
        tmux_session_name: row.try_get("tmux_session_name")?,
        worktree_path: row.try_get("worktree_path")?,
        tmux_status: row.try_get("tmux_status")?,
        status_source: row.try_get("status_source")?,
        status_fetched_at: row.try_get("status_fetched_at")?,
        status_error: row.try_get("status_error")?,
        opencode_session_id: row.try_get("opencode_session_id")?,
        attach_overlay_shown: row.try_get::<i64, _>("attach_overlay_shown")? != 0,
        archived: row.try_get::<i64, _>("archived")? != 0,
        archived_at: row.try_get("archived_at")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn is_duplicate_column_err(err: &sqlx::Error) -> bool {
    match err {
        sqlx::Error::Database(database_err) => {
            database_err.message().contains("duplicate column name")
        }
        _ => false,
    }
}

fn parse_uuid_column(value: String) -> Result<Uuid> {
    Uuid::parse_str(&value).with_context(|| format!("invalid UUID value in sqlite row: {value}"))
}

fn now_iso() -> String {
    Utc::now().to_rfc3339()
}

fn normalize_category_slug(value: &str) -> String {
    let mut slug = String::new();
    let mut previous_dash = false;

    for ch in value.trim().chars() {
        let normalized = ch.to_ascii_lowercase();
        if normalized.is_ascii_alphanumeric() {
            slug.push(normalized);
            previous_dash = false;
            continue;
        }

        if !previous_dash {
            slug.push('-');
            previous_dash = true;
        }
    }

    while slug.starts_with('-') {
        slug.remove(0);
    }

    while slug.ends_with('-') {
        slug.pop();
    }

    if slug.is_empty() {
        "category".to_string()
    } else {
        slug
    }
}

fn next_available_slug(base: &str, used: &HashSet<String>) -> String {
    if !used.contains(base) {
        return base.to_string();
    }

    let mut index = 2;
    loop {
        let candidate = format!("{base}-{index}");
        if !used.contains(&candidate) {
            return candidate;
        }
        index += 1;
    }
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
    use uuid::Uuid;

    use super::Database;

    #[test]
    fn test_db_creation_seeds_default_categories() -> Result<()> {
        let db = Database::open(":memory:")?;
        let categories = db.list_categories()?;

        assert_eq!(categories.len(), 3);
        assert_eq!(categories[0].slug, "todo");
        assert_eq!(categories[1].slug, "in-progress");
        assert_eq!(categories[2].slug, "done");
        assert_eq!(categories[0].name, "TODO");
        assert_eq!(categories[1].name, "IN PROGRESS");
        assert_eq!(categories[2].name, "DONE");
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
    fn test_repo_and_task_crud() -> Result<()> {
        let db = Database::open(":memory:")?;
        let repo_dir = create_temp_git_repo("task-crud")?;
        let repo = db.add_repo(&repo_dir)?;
        let categories = db.list_categories()?;
        let todo_category = categories[0].id;

        let task = db.add_task(repo.id, "feature/db-layer", "", todo_category)?;
        assert_eq!(task.title, "feature/db-layer");
        assert_eq!(task.tmux_status, "unknown");
        assert_eq!(task.status_source, "none");
        assert!(!task.attach_overlay_shown);
        assert!(!task.archived);
        assert_eq!(task.archived_at, None);

        db.update_task_status(task.id, "running")?;
        db.update_task_title(task.id, "Renamed DB Task")?;
        db.update_task_status_metadata(
            task.id,
            "tmux",
            Some("2026-02-15T12:34:56Z".to_string()),
            Some("transient timeout".to_string()),
        )?;
        db.update_task_session_binding(task.id, Some("sid-task-crud".to_string()))?;

        let updated = db.get_task(task.id)?;
        assert_eq!(updated.title, "Renamed DB Task");
        assert_eq!(updated.tmux_status, "running");
        assert_eq!(updated.status_source, "tmux");
        assert_eq!(
            updated.opencode_session_id.as_deref(),
            Some("sid-task-crud")
        );

        db.update_task_attach_overlay_shown(task.id, true)?;
        let updated = db.get_task(task.id)?;
        assert!(updated.attach_overlay_shown);

        db.delete_task(task.id)?;
        assert!(db.get_task(task.id).is_err());

        std::fs::remove_dir_all(&repo_dir)?;
        Ok(())
    }

    #[test]
    fn test_archive_and_unarchive_task_visibility() -> Result<()> {
        let db = Database::open(":memory:")?;
        let repo_dir = create_temp_git_repo("archive-visibility")?;
        let repo = db.add_repo(&repo_dir)?;
        let category_id = db.list_categories()?[0].id;

        let task = db.add_task(repo.id, "feature/archive", "Archive Me", category_id)?;
        assert_eq!(db.list_tasks()?.len(), 1);
        assert_eq!(db.list_archived_tasks()?.len(), 0);

        db.archive_task(task.id)?;
        assert_eq!(db.list_tasks()?.len(), 0);
        let archived = db.list_archived_tasks()?;
        assert_eq!(archived.len(), 1);
        assert!(archived[0].archived);
        assert!(archived[0].archived_at.is_some());

        db.unarchive_task(task.id)?;
        let visible = db.list_tasks()?;
        assert_eq!(visible.len(), 1);
        assert!(!visible[0].archived);
        assert_eq!(visible[0].archived_at, None);
        assert_eq!(db.list_archived_tasks()?.len(), 0);

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
    fn test_rename_category_updates_slug() -> Result<()> {
        let db = Database::open(":memory:")?;
        let todo = db
            .get_category_by_slug("todo")?
            .expect("todo category should exist");

        db.rename_category(todo.id, "Code Review")?;

        let categories = db.list_categories()?;
        let renamed = categories
            .iter()
            .find(|category| category.id == todo.id)
            .expect("renamed category should still exist");
        assert_eq!(renamed.name, "Code Review");
        assert_eq!(renamed.slug, "code-review");

        let by_new_slug = db
            .get_category_by_slug("code-review")?
            .expect("updated slug should resolve category");
        assert_eq!(by_new_slug.id, todo.id);
        assert!(db.get_category_by_slug("todo")?.is_none());

        Ok(())
    }

    #[test]
    fn test_command_frequency() -> Result<()> {
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
