#![allow(dead_code)]
#![allow(unused_imports)]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::thread;
use std::time::Duration;
use std::{collections::VecDeque, env, io::Read, io::Write, net::TcpListener, time::SystemTime};

use anyhow::{Context, Result, bail};
use tempfile::TempDir;

use opencode_kanban::app::App;
use opencode_kanban::db::Database;
use opencode_kanban::git::{
    git_create_worktree, git_delete_branch, git_fetch, git_remove_worktree,
};
use opencode_kanban::opencode::{OpenCodeBindingState, Status, classify_binding_state};
use opencode_kanban::tmux::{
    sanitize_session_name, tmux_create_session, tmux_kill_session, tmux_session_exists,
};
use opencode_kanban::types::{
    SessionState, SessionStatus, SessionStatusError, SessionStatusSource, Task,
};

static INTEGRATION_TEST_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

#[test]
fn integration_test_full_lifecycle() -> Result<()> {
    if !tmux_available() {
        return Ok(());
    }
    let _test_guard = INTEGRATION_TEST_LOCK
        .lock()
        .expect("integration test lock should not be poisoned");

    let socket = format!("ok-integration-{}", std::process::id());
    let _socket_guard = EnvVarGuard::set("OPENCODE_KANBAN_TMUX_SOCKET", &socket);

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
        Some(worktree_path.display().to_string()),
    )?;

    thread::sleep(Duration::from_millis(250));
    db.update_task_status(task.id, Status::Idle.as_str())?;

    let created = db.get_task(task.id)?;
    assert_eq!(
        created.tmux_session_name.as_deref(),
        Some(session_name.as_str())
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
    Ok(())
}

#[test]
fn integration_test_server_first_lifecycle_with_stale_binding_transition() -> Result<()> {
    if !tmux_available() {
        return Ok(());
    }
    let _test_guard = INTEGRATION_TEST_LOCK
        .lock()
        .expect("integration test lock should not be poisoned");

    let fixture = GitFixture::new()?;
    let socket = format!("ok-server-first-{}", std::process::id());
    let _socket_guard = EnvVarGuard::set("OPENCODE_KANBAN_TMUX_SOCKET", &socket);

    let xdg_data_home = fixture.temp.path().join("xdg-server-first");
    let _xdg_guard = EnvVarGuard::set("XDG_DATA_HOME", xdg_data_home.display().to_string());

    cleanup_test_tmux_server();

    let mock_server = MockStatusServer::start(
        vec![
            http_json_response(&format!(
                "[{{\"id\":\"sid-server-first\",\"directory\":\"{}\"}}]",
                fixture.repo_path().display()
            )),
            http_json_response(&format!(
                "[{{\"id\":\"sid-server-first\",\"directory\":\"{}\"}}]",
                fixture.repo_path().display()
            )),
            http_json_response(&format!(
                "[{{\"id\":\"sid-server-first\",\"directory\":\"{}\"}}]",
                fixture.repo_path().display()
            )),
            http_json_response("[]"),
        ],
        vec![
            http_json_response("{\"sid-server-first\":{\"state\":\"running\"}}"),
            http_json_response("{\"sid-server-first\":{\"state\":\"running\"}}"),
            http_json_response("{\"sid-server-first\":{\"state\":\"running\"}}"),
            http_json_response("{}"),
        ],
    )?;
    let _port_guard = EnvVarGuard::set("OPENCODE_KANBAN_STATUS_PORT", mock_server.port.to_string());

    let db_path = kanban_db_path(&xdg_data_home);
    let db = Database::open(&db_path)?;
    let repo = db.add_repo(fixture.repo_path())?;
    let todo = db.list_categories()?[0].id;

    let session_name = sanitize_session_name(&repo.name, "feature/server-first-lifecycle");
    tmux_create_session(
        &session_name,
        fixture.repo_path(),
        Some("printf \"thinking...\\n\"; sleep 30"),
    )?;

    let task = db.add_task(
        repo.id,
        "feature/server-first-lifecycle",
        "Server-first lifecycle",
        todo,
    )?;
    db.update_task_tmux(
        task.id,
        Some(session_name.clone()),
        Some(fixture.repo_path().display().to_string()),
    )?;

    {
        let _app = App::new(None)?;

        wait_for_task(&db_path, task.id, Duration::from_secs(12), |current| {
            current.status_source == "server" && current.status_error.is_none()
        })?;

        wait_for_task(&db_path, task.id, Duration::from_secs(12), |current| {
            current.status_source == "none"
                && current.tmux_status == "dead"
                && current
                    .status_error
                    .as_deref()
                    .is_some_and(|error| error.starts_with("SESSION_NOT_FOUND:"))
        })?;
    }

    let updated = Database::open(&db_path)?.get_task(task.id)?;
    assert_eq!(
        binding_state_from_task(&updated),
        OpenCodeBindingState::Stale,
        "missing server session should be treated as stale binding"
    );

    tmux_kill_session(&session_name)?;
    cleanup_test_tmux_server();
    Ok(())
}

#[test]
fn integration_test_server_failure_falls_back_to_tmux_across_poll_cycles() -> Result<()> {
    if !tmux_available() {
        return Ok(());
    }
    let _test_guard = INTEGRATION_TEST_LOCK
        .lock()
        .expect("integration test lock should not be poisoned");

    let fixture = GitFixture::new()?;
    let socket = format!("ok-fallback-{}", std::process::id());
    let _socket_guard = EnvVarGuard::set("OPENCODE_KANBAN_TMUX_SOCKET", &socket);

    let xdg_data_home = fixture.temp.path().join("xdg-fallback");
    let _xdg_guard = EnvVarGuard::set("XDG_DATA_HOME", xdg_data_home.display().to_string());

    cleanup_test_tmux_server();

    let mock_server = MockStatusServer::start(
        vec![http_json_response("[]")],
        vec![http_error_response(500)],
    )?;
    let _port_guard = EnvVarGuard::set("OPENCODE_KANBAN_STATUS_PORT", mock_server.port.to_string());

    let db_path = kanban_db_path(&xdg_data_home);
    let db = Database::open(&db_path)?;
    let repo = db.add_repo(fixture.repo_path())?;
    let todo = db.list_categories()?[0].id;

    let session_name = sanitize_session_name(&repo.name, "feature/fallback-lifecycle");
    tmux_create_session(&session_name, fixture.repo_path(), Some("sleep 30"))?;

    let task = db.add_task(
        repo.id,
        "feature/fallback-lifecycle",
        "Fallback lifecycle",
        todo,
    )?;
    db.update_task_tmux(
        task.id,
        Some(session_name.clone()),
        Some(fixture.repo_path().display().to_string()),
    )?;

    {
        let _app = App::new(None)?;

        wait_for_task(&db_path, task.id, Duration::from_secs(12), |current| {
            current.tmux_status == "dead"
                && current.status_source == "none"
                && current
                    .status_error
                    .as_deref()
                    .is_some_and(|error| error.starts_with("SERVER_"))
        })?;
    }

    tmux_kill_session(&session_name)?;
    cleanup_test_tmux_server();
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
            [
                "init",
                "--bare",
                "-b",
                "main",
                origin.to_string_lossy().as_ref(),
            ],
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

fn kanban_db_path(xdg_data_home: &Path) -> PathBuf {
    xdg_data_home
        .join("opencode-kanban")
        .join("opencode-kanban.sqlite")
}

fn wait_for_task(
    db_path: &Path,
    task_id: uuid::Uuid,
    timeout: Duration,
    predicate: impl Fn(&Task) -> bool,
) -> Result<()> {
    let start = std::time::Instant::now();

    while start.elapsed() <= timeout {
        if let Ok(db) = Database::open(db_path)
            && let Ok(task) = db.get_task(task_id)
        {
            if predicate(&task) {
                return Ok(());
            }
        }

        thread::sleep(Duration::from_millis(120));
    }

    let task = Database::open(db_path)?.get_task(task_id)?;
    bail!(
        "timed out waiting for task {} after {:?}; observed status='{}' source='{}' error={:?}",
        task_id,
        timeout,
        task.tmux_status,
        task.status_source,
        task.status_error
    )
}

fn binding_state_from_task(task: &Task) -> OpenCodeBindingState {
    let source = match task.status_source.as_str() {
        "server" => SessionStatusSource::Server,
        _ => SessionStatusSource::None,
    };

    let status = SessionStatus {
        state: SessionState::Idle,
        source,
        fetched_at: SystemTime::now(),
        error: task.status_error.as_ref().map(|raw| SessionStatusError {
            code: raw
                .split(':')
                .next()
                .map(str::trim)
                .unwrap_or_default()
                .to_string(),
            message: raw.clone(),
        }),
    };

    classify_binding_state(None, Some(&status))
}

fn http_json_response(body: &str) -> String {
    format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{body}")
}

fn http_error_response(status_code: u16) -> String {
    format!(
        "HTTP/1.1 {status_code} Error\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{{\"error\":\"mock\"}}"
    )
}

struct MockStatusServer {
    stop: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
    session_responses: Arc<Mutex<VecDeque<String>>>,
    session_status_responses: Arc<Mutex<VecDeque<String>>>,
    pub port: u16,
}

impl MockStatusServer {
    fn start(
        session_responses: Vec<String>,
        session_status_responses: Vec<String>,
    ) -> Result<Self> {
        // Bind to port 0 to get an OS-assigned available port.
        // This avoids conflicts with local OpenCode processes on port 4096.
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .context("failed to bind mock status server on random available port")?;
        let port = listener
            .local_addr()
            .context("failed to get mock server port")?
            .port();
        listener
            .set_nonblocking(true)
            .context("failed to make mock status server non-blocking")?;

        let stop = Arc::new(AtomicBool::new(false));
        let stop_flag = Arc::clone(&stop);
        let session_responses = Arc::new(Mutex::new(VecDeque::from(session_responses)));
        let session_response_queue = Arc::clone(&session_responses);
        let session_status_responses =
            Arc::new(Mutex::new(VecDeque::from(session_status_responses)));
        let session_status_response_queue = Arc::clone(&session_status_responses);

        let handle = thread::spawn(move || {
            while !stop_flag.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let _ = stream.set_read_timeout(Some(Duration::from_millis(150)));
                        let mut request = [0u8; 2048];
                        let read = match stream.read(&mut request) {
                            Ok(read) => read,
                            Err(err)
                                if err.kind() == std::io::ErrorKind::WouldBlock
                                    || err.kind() == std::io::ErrorKind::TimedOut =>
                            {
                                0
                            }
                            Err(_) => 0,
                        };
                        let request = String::from_utf8_lossy(&request[..read]);

                        let response = if request.starts_with("GET /global/health") {
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{\"healthy\":true}"
                                .to_string()
                        } else if request.starts_with("GET /session")
                            && !request.starts_with("GET /session/status")
                        {
                            session_response_queue
                                .lock()
                                .expect("mock session response queue lock should not be poisoned")
                                .pop_front()
                                .unwrap_or_else(|| "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n[]".to_string())
                        } else if request.starts_with("GET /session/status") {
                            session_status_response_queue
                                .lock()
                                .expect("mock session status response queue lock should not be poisoned")
                                .pop_front()
                                .unwrap_or_else(|| "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{}".to_string())
                        } else {
                            "HTTP/1.1 404 Not Found\r\nConnection: close\r\n\r\n".to_string()
                        };

                        let _ = stream.write_all(response.as_bytes());
                        let _ = stream.flush();
                    }
                    Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(15));
                    }
                    Err(_) => break,
                }
            }
        });

        Ok(Self {
            stop,
            handle: Some(handle),
            session_responses,
            session_status_responses,
            port,
        })
    }
}

impl Drop for MockStatusServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

struct EnvVarGuard {
    key: &'static str,
    previous: Option<String>,
}

impl EnvVarGuard {
    fn set(key: &'static str, value: impl AsRef<str>) -> Self {
        let previous = env::var(key).ok();
        unsafe {
            env::set_var(key, value.as_ref());
        }
        Self { key, previous }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        if let Some(previous) = &self.previous {
            unsafe {
                env::set_var(self.key, previous);
            }
        } else {
            unsafe {
                env::remove_var(self.key);
            }
        }
    }
}
