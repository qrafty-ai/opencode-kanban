//! Runtime traits and implementations for git/tmux operations

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::git::{
    git_check_branch_up_to_date, git_create_worktree, git_detect_default_branch, git_fetch,
    git_is_valid_repo, git_remove_worktree,
};
use crate::tmux::{
    PopupThemeStyle, sanitize_session_name_for_project, tmux_create_session, tmux_kill_session,
    tmux_session_exists, tmux_show_popup, tmux_switch_client,
};

/// Runtime trait for task recovery operations
pub trait RecoveryRuntime {
    fn repo_exists(&self, path: &Path) -> bool;
    fn worktree_exists(&self, worktree_path: &Path) -> bool;
    fn session_exists(&self, session_name: &str) -> bool;
    fn create_session(&self, session_name: &str, working_dir: &Path, command: &str) -> Result<()>;
    fn switch_client(
        &self,
        session_name: &str,
        reopen_lines: &[String],
        style: &PopupThemeStyle,
    ) -> Result<()>;
    fn show_attach_popup(&self, lines: &[String], style: &PopupThemeStyle) -> Result<()>;
}

/// Real implementation of RecoveryRuntime using actual git/tmux commands
pub struct RealRecoveryRuntime;

impl RecoveryRuntime for RealRecoveryRuntime {
    fn repo_exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn worktree_exists(&self, worktree_path: &Path) -> bool {
        worktree_path.exists()
    }

    fn session_exists(&self, session_name: &str) -> bool {
        tmux_session_exists(session_name)
    }

    fn create_session(&self, session_name: &str, working_dir: &Path, command: &str) -> Result<()> {
        tmux_create_session(session_name, working_dir, Some(command))
    }

    fn switch_client(
        &self,
        session_name: &str,
        reopen_lines: &[String],
        style: &PopupThemeStyle,
    ) -> Result<()> {
        tmux_switch_client(session_name, reopen_lines, style)
    }

    fn show_attach_popup(&self, lines: &[String], style: &PopupThemeStyle) -> Result<()> {
        tmux_show_popup(lines, style)
    }
}

/// Runtime trait for task creation operations
pub trait CreateTaskRuntime {
    fn git_is_valid_repo(&self, path: &Path) -> bool;
    fn git_detect_default_branch(&self, repo_path: &Path) -> String;
    fn git_fetch(&self, repo_path: &Path) -> Result<()>;
    fn git_validate_branch(&self, repo_path: &Path, branch_name: &str) -> Result<()>;
    fn git_check_branch_up_to_date(&self, repo_path: &Path, base_ref: &str) -> Result<()>;
    fn git_create_worktree(
        &self,
        repo_path: &Path,
        worktree_path: &Path,
        branch_name: &str,
        base_ref: &str,
    ) -> Result<()>;
    fn git_remove_worktree(&self, repo_path: &Path, worktree_path: &Path) -> Result<()>;
    fn tmux_session_exists(&self, session_name: &str) -> bool;
    fn tmux_create_session(
        &self,
        session_name: &str,
        working_dir: &Path,
        command: Option<&str>,
    ) -> Result<()>;
    fn tmux_kill_session(&self, session_name: &str) -> Result<()>;
}

/// Real implementation of CreateTaskRuntime using actual git/tmux commands
pub struct RealCreateTaskRuntime;

impl CreateTaskRuntime for RealCreateTaskRuntime {
    fn git_is_valid_repo(&self, path: &Path) -> bool {
        git_is_valid_repo(path)
    }

    fn git_detect_default_branch(&self, repo_path: &Path) -> String {
        git_detect_default_branch(repo_path)
    }

    fn git_fetch(&self, repo_path: &Path) -> Result<()> {
        git_fetch(repo_path)
    }

    fn git_validate_branch(&self, repo_path: &Path, branch_name: &str) -> Result<()> {
        let output = std::process::Command::new("git")
            .args(["check-ref-format", "--branch", branch_name])
            .current_dir(repo_path)
            .output()
            .with_context(|| {
                format!(
                    "failed to validate branch name `{branch_name}` in {}",
                    repo_path.display()
                )
            })?;

        if output.status.success() {
            Ok(())
        } else {
            anyhow::bail!(
                "invalid branch name `{branch_name}`\nstdout: {}\nstderr: {}",
                String::from_utf8_lossy(&output.stdout).trim(),
                String::from_utf8_lossy(&output.stderr).trim()
            )
        }
    }

    fn git_check_branch_up_to_date(&self, repo_path: &Path, base_ref: &str) -> Result<()> {
        git_check_branch_up_to_date(repo_path, base_ref)
    }

    fn git_create_worktree(
        &self,
        repo_path: &Path,
        worktree_path: &Path,
        branch_name: &str,
        base_ref: &str,
    ) -> Result<()> {
        git_create_worktree(repo_path, worktree_path, branch_name, base_ref)
    }

    fn git_remove_worktree(&self, repo_path: &Path, worktree_path: &Path) -> Result<()> {
        git_remove_worktree(repo_path, worktree_path)
    }

    fn tmux_session_exists(&self, session_name: &str) -> bool {
        tmux_session_exists(session_name)
    }

    fn tmux_create_session(
        &self,
        session_name: &str,
        working_dir: &Path,
        command: Option<&str>,
    ) -> Result<()> {
        tmux_create_session(session_name, working_dir, command)
    }

    fn tmux_kill_session(&self, session_name: &str) -> Result<()> {
        tmux_kill_session(session_name)
    }
}

/// Generate next available tmux session name
pub fn next_available_session_name(
    existing_name: Option<&str>,
    project_slug: Option<&str>,
    repo_name: &str,
    branch_name: &str,
    runtime: &impl RecoveryRuntime,
) -> String {
    next_available_session_name_by(
        existing_name,
        project_slug,
        repo_name,
        branch_name,
        |name| runtime.session_exists(name),
    )
}

/// Generate next available session name with custom existence check
pub fn next_available_session_name_by<F>(
    existing_name: Option<&str>,
    project_slug: Option<&str>,
    repo_name: &str,
    branch_name: &str,
    session_exists: F,
) -> String
where
    F: Fn(&str) -> bool,
{
    if let Some(existing_name) = existing_name
        && !session_exists(existing_name)
    {
        return existing_name.to_string();
    }

    let base = sanitize_session_name_for_project(project_slug, repo_name, branch_name);
    if !session_exists(&base) {
        return base;
    }

    for suffix in 2..10_000 {
        let candidate = format!("{base}-{suffix}");
        if !session_exists(&candidate) {
            return candidate;
        }
    }

    base
}

/// Get worktrees root directory for a repo
pub fn worktrees_root_for_repo(repo_path: &Path) -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| {
            repo_path
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| PathBuf::from("."))
        })
        .join(".opencode-kanban-worktrees")
}
