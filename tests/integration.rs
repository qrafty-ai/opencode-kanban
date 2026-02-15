#![allow(dead_code)]
#![allow(unused_imports)]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result};
use tempfile::TempDir;

use opencode_kanban::db::Database;
use opencode_kanban::git::{
    git_create_worktree, git_delete_branch, git_fetch, git_remove_worktree,
};
use opencode_kanban::opencode::{Status, opencode_detect_status};
use opencode_kanban::tmux::{
    sanitize_session_name, tmux_capture_pane, tmux_create_session, tmux_kill_session,
    tmux_session_exists,
};

#[test]
fn integration_test_full_lifecycle() -> Result<()> {
    if !tmux_available() {
        return Ok(());
    }

    let socket = format!("ok-integration-{}", std::process::id());
    unsafe {
        std::env::set_var("OPENCODE_KANBAN_TMUX_SOCKET", &socket);
    }

    cleanup_test_tmux_server();

    let fixture = GitFixture::new()?;
    let db_path = fixture.temp.path().join("kanban.sqlite");
    let db = Database::open(&db_path)?;
    let repo = db.add_repo(fixture.repo_path())?;
    let categories = db.list_categories()?;
    let todo = categories[0].id;
    let in_progress = categories[1].id;

    let branch = "feature/integration-lifecycle";
    let worktree_path = fixture
        .temp
        .path()
        .join("worktrees")
        .join("integration-lifecycle");

    git_fetch(fixture.repo_path())?;
    git_create_worktree(fixture.repo_path(), &worktree_path, branch, "origin/main")?;
    assert!(worktree_path.exists());

    let session_name = sanitize_session_name(&repo.name, branch);
    tmux_create_session(
        &session_name,
        &worktree_path,
        Some("printf \"I'm ready\\n\"; sleep 30"),
    )?;
    assert!(tmux_session_exists(&session_name));

    let task = db.add_task(repo.id, branch, "Lifecycle task", todo)?;
    db.update_task_tmux(
        task.id,
        Some(session_name.clone()),
        None,
        Some(worktree_path.display().to_string()),
    )?;

    thread::sleep(Duration::from_millis(250));
    let pane = tmux_capture_pane(&session_name, 60)?;
    let status = opencode_detect_status(&pane);
    db.update_task_status(task.id, status.as_str())?;

    let created = db.get_task(task.id)?;
    assert_eq!(
        created.tmux_session_name.as_deref(),
        Some(session_name.as_str())
    );
    assert!(
        matches!(status, Status::Idle | Status::Unknown),
        "unexpected status: {status:?}"
    );

    db.update_task_category(task.id, in_progress, 0)?;
    let moved = db.get_task(task.id)?;
    assert_eq!(moved.category_id, in_progress);

    tmux_kill_session(&session_name)?;
    assert!(!tmux_session_exists(&session_name));

    git_remove_worktree(fixture.repo_path(), &worktree_path)?;
    assert!(!worktree_path.exists());
    git_delete_branch(fixture.repo_path(), branch)?;

    let branches = git_stdout(fixture.repo_path(), ["branch", "--format=%(refname:short)"])?;
    assert!(!branches.lines().any(|line| line.trim() == branch));

    db.delete_task(task.id)?;
    assert!(db.get_task(task.id).is_err());

    cleanup_test_tmux_server();
    unsafe {
        std::env::remove_var("OPENCODE_KANBAN_TMUX_SOCKET");
    }
    Ok(())
}

struct GitFixture {
    temp: TempDir,
    repo: PathBuf,
}

impl GitFixture {
    fn new() -> Result<Self> {
        let temp = TempDir::new()?;
        let origin = temp.path().join("origin.git");
        let seed = temp.path().join("seed");
        let repo = temp.path().join("repo");

        std::fs::create_dir_all(&seed)?;
        run_git(
            temp.path(),
            ["init", "--bare", origin.to_string_lossy().as_ref()],
        )?;

        run_git(
            temp.path(),
            ["init", "-b", "main", seed.to_string_lossy().as_ref()],
        )?;
        run_git(&seed, ["config", "user.name", "Test User"])?;
        run_git(&seed, ["config", "user.email", "test@example.com"])?;
        run_git(&seed, ["commit", "--allow-empty", "-m", "init"])?;
        run_git(
            &seed,
            ["remote", "add", "origin", origin.to_string_lossy().as_ref()],
        )?;
        run_git(&seed, ["push", "-u", "origin", "main"])?;

        run_git(
            temp.path(),
            [
                "clone",
                origin.to_string_lossy().as_ref(),
                repo.to_string_lossy().as_ref(),
            ],
        )?;
        run_git(&repo, ["config", "user.name", "Test User"])?;
        run_git(&repo, ["config", "user.email", "test@example.com"])?;

        Ok(Self { temp, repo })
    }

    fn repo_path(&self) -> &Path {
        &self.repo
    }
}

fn run_git<I, S>(cwd: &Path, args: I) -> Result<()>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let args_vec: Vec<String> = args
        .into_iter()
        .map(|arg| arg.as_ref().to_string())
        .collect();
    let output = Command::new("git")
        .args(args_vec.iter().map(String::as_str))
        .current_dir(cwd)
        .output()
        .with_context(|| format!("failed to run git {}", args_vec.join(" ")))?;

    if output.status.success() {
        Ok(())
    } else {
        anyhow::bail!(
            "git command failed in {}: git {}\nstdout: {}\nstderr: {}",
            cwd.display(),
            args_vec.join(" "),
            String::from_utf8_lossy(&output.stdout).trim(),
            String::from_utf8_lossy(&output.stderr).trim()
        )
    }
}

fn git_stdout<I, S>(cwd: &Path, args: I) -> Result<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let args_vec: Vec<String> = args
        .into_iter()
        .map(|arg| arg.as_ref().to_string())
        .collect();
    let output = Command::new("git")
        .args(args_vec.iter().map(String::as_str))
        .current_dir(cwd)
        .output()
        .with_context(|| format!("failed to run git {}", args_vec.join(" ")))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        anyhow::bail!(
            "git command failed in {}: git {}\nstdout: {}\nstderr: {}",
            cwd.display(),
            args_vec.join(" "),
            String::from_utf8_lossy(&output.stdout).trim(),
            String::from_utf8_lossy(&output.stderr).trim()
        )
    }
}

fn tmux_available() -> bool {
    Command::new("tmux")
        .arg("-V")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn cleanup_test_tmux_server() {
    let socket = std::env::var("OPENCODE_KANBAN_TMUX_SOCKET")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "opencode-kanban-test".to_string());
    let _ = Command::new("tmux")
        .args(["-L", socket.as_str(), "kill-server"])
        .output();
}
