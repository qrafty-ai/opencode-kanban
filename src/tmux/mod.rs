#![allow(dead_code)]

use std::env;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail};

const TMUX_SOCKET: &str = "";

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct TmuxSession {
    pub name: String,
    pub created_at: i64,
    pub attached: bool,
}

pub fn ensure_tmux_installed() -> Result<()> {
    let output = Command::new("tmux")
        .arg("-V")
        .output()
        .context("failed to execute tmux")?;

    if output.status.success() {
        Ok(())
    } else {
        bail!(
            "tmux is required but not available. Install tmux and ensure it is on PATH, then retry."
        )
    }
}

pub fn tmux_session_exists(session_name: &str) -> bool {
    tmux_command()
        .args(has_session_args(session_name))
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

pub fn tmux_create_session(
    session_name: &str,
    working_dir: &Path,
    command: Option<&str>,
) -> Result<()> {
    let mut args = new_session_args(session_name, working_dir);
    if let Some(command) = command {
        args.push(command.to_string());
    }

    let output = tmux_command()
        .args(args)
        .output()
        .context("failed to run tmux new-session")?;

    ensure_success(&output, "new-session")?;

    Ok(())
}

pub fn tmux_kill_session(session_name: &str) -> Result<()> {
    let output = tmux_command()
        .args(kill_session_args(session_name))
        .output()
        .context("failed to run tmux kill-session")?;
    ensure_success(&output, "kill-session")
}

pub fn tmux_switch_client(session_name: &str) -> Result<()> {
    tmux_ensure_return_binding()?;

    let output = Command::new("tmux")
        .args(switch_client_args(session_name))
        .output()
        .context("failed to run tmux switch-client")?;
    ensure_success_with_output(&output, "switch-client")
}

fn tmux_ensure_return_binding() -> Result<()> {
    let output = Command::new("tmux")
        .args(return_binding_args())
        .output()
        .context("failed to configure return key binding")?;
    ensure_success_with_output(&output, "bind-key")
}

fn ensure_success_with_output(output: &std::process::Output, command: &str) -> Result<()> {
    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        bail!(
            "tmux {} failed: {}{}",
            command,
            stderr.trim(),
            if stdout.is_empty() {
                String::new()
            } else {
                format!(" {}", stdout.trim())
            }
        )
    }
}

pub fn tmux_list_sessions() -> Vec<TmuxSession> {
    let output = tmux_command().args(list_sessions_args()).output();

    let Ok(output) = output else {
        return Vec::new();
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("no server running") || stderr.contains("failed to connect") {
            return Vec::new();
        }
        return Vec::new();
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut sessions = Vec::new();

    for line in stdout.lines().filter(|line| !line.trim().is_empty()) {
        let mut parts = line.split('\t');
        let Some(name) = parts.next() else {
            continue;
        };
        let Some(created_raw) = parts.next() else {
            continue;
        };
        let Some(attached_raw) = parts.next() else {
            continue;
        };

        let created_at = created_raw.parse::<i64>().unwrap_or_default();
        let attached = attached_raw == "1";

        sessions.push(TmuxSession {
            name: name.to_string(),
            created_at,
            attached,
        });
    }

    sessions
}

pub fn tmux_capture_pane(session_name: &str, lines: usize) -> Result<String> {
    let start = format!("-{lines}");
    let output = tmux_command()
        .args(capture_pane_window_args(session_name, &start))
        .output()
        .context("failed to run tmux capture-pane")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("can't find window: 0") {
            let retry = tmux_command()
                .args(capture_pane_session_args(session_name, &start))
                .output()
                .context("failed to rerun tmux capture-pane")?;
            ensure_success(&retry, "capture-pane")?;
            return Ok(String::from_utf8_lossy(&retry.stdout).to_string());
        }
    }

    ensure_success(&output, "capture-pane")
        .map(|_| String::from_utf8_lossy(&output.stdout).to_string())
}

pub fn tmux_get_pane_pid(session_name: &str) -> Option<u32> {
    let output = tmux_command()
        .args(list_panes_args(session_name))
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())?
        .parse()
        .ok()
}

pub fn sanitize_session_name(repo_name: &str, branch_name: &str) -> String {
    let repo = sanitize_fragment(repo_name);
    let branch = sanitize_fragment(branch_name);
    let mut session_name = format!("ok-{repo}-{branch}");
    session_name.truncate(200);
    session_name
}

pub fn sanitize_session_name_for_project(
    project_slug: Option<&str>,
    repo_name: &str,
    branch_name: &str,
) -> String {
    let Some(project_slug) = project_slug
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return sanitize_session_name(repo_name, branch_name);
    };

    let project = sanitize_fragment(project_slug);
    let repo = sanitize_fragment(repo_name);
    let branch = sanitize_fragment(branch_name);
    let mut session_name = format!("ok-{project}-{repo}-{branch}");
    session_name.truncate(200);
    session_name
}

fn sanitize_fragment(input: &str) -> String {
    input
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' {
                ch
            } else {
                '-'
            }
        })
        .collect()
}

fn tmux_command() -> Command {
    let mut cmd = Command::new("tmux");
    let socket = tmux_socket();
    if !socket.is_empty() {
        cmd.args(socket_args(&socket));
    }
    cmd
}

fn socket_args(socket: &str) -> Vec<String> {
    vec!["-L".to_string(), socket.to_string()]
}

fn has_session_args(session_name: &str) -> Vec<String> {
    vec![
        "has-session".to_string(),
        "-t".to_string(),
        session_name.to_string(),
    ]
}

fn new_session_args(session_name: &str, working_dir: &Path) -> Vec<String> {
    vec![
        "new-session".to_string(),
        "-d".to_string(),
        "-s".to_string(),
        session_name.to_string(),
        "-c".to_string(),
        working_dir.to_string_lossy().to_string(),
    ]
}

fn kill_session_args(session_name: &str) -> Vec<String> {
    vec![
        "kill-session".to_string(),
        "-t".to_string(),
        session_name.to_string(),
    ]
}

fn switch_client_args(session_name: &str) -> Vec<String> {
    vec![
        "switch-client".to_string(),
        "-t".to_string(),
        session_name.to_string(),
    ]
}

fn return_binding_args() -> Vec<String> {
    vec![
        "bind-key".to_string(),
        "-T".to_string(),
        "prefix".to_string(),
        "K".to_string(),
        "switch-client".to_string(),
        "-l".to_string(),
    ]
}

fn list_sessions_args() -> Vec<String> {
    vec![
        "list-sessions".to_string(),
        "-F".to_string(),
        "#{session_name}\t#{session_created}\t#{session_attached}".to_string(),
    ]
}

fn capture_pane_window_args(session_name: &str, start: &str) -> Vec<String> {
    vec![
        "capture-pane".to_string(),
        "-t".to_string(),
        format!("{session_name}:0.0"),
        "-p".to_string(),
        "-S".to_string(),
        start.to_string(),
    ]
}

fn capture_pane_session_args(session_name: &str, start: &str) -> Vec<String> {
    vec![
        "capture-pane".to_string(),
        "-t".to_string(),
        session_name.to_string(),
        "-p".to_string(),
        "-S".to_string(),
        start.to_string(),
    ]
}

fn list_panes_args(session_name: &str) -> Vec<String> {
    vec![
        "list-panes".to_string(),
        "-t".to_string(),
        session_name.to_string(),
        "-F".to_string(),
        "#{pane_pid}".to_string(),
    ]
}

fn tmux_socket() -> String {
    if let Ok(socket) = env::var("OPENCODE_KANBAN_TMUX_SOCKET") {
        let trimmed = socket.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }

    if cfg!(test) {
        format!("opencode-kanban-test-{}", std::process::id())
    } else {
        TMUX_SOCKET.to_string()
    }
}

fn ensure_success(output: &std::process::Output, operation: &str) -> Result<()> {
    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    bail!("tmux {operation} failed: {}", stderr.trim())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use std::thread;
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

    struct SessionCleanup {
        name: String,
    }

    impl SessionCleanup {
        fn new(name: String) -> Self {
            Self { name }
        }
    }

    impl Drop for SessionCleanup {
        fn drop(&mut self) {
            if tmux_session_exists(&self.name) {
                let _ = tmux_kill_session(&self.name);
            }
        }
    }

    fn wait_for_session(session_name: &str, timeout: Duration) -> bool {
        let start = Instant::now();
        while start.elapsed() < timeout {
            if tmux_session_exists(session_name) {
                return true;
            }
            thread::sleep(Duration::from_millis(50));
        }
        false
    }

    fn wait_for_pane_pid(session_name: &str, timeout: Duration) -> Option<u32> {
        let start = Instant::now();
        while start.elapsed() < timeout {
            if let Some(pid) = tmux_get_pane_pid(session_name) {
                return Some(pid);
            }
            thread::sleep(Duration::from_millis(50));
        }
        None
    }

    #[test]
    fn test_sanitize_session_name() {
        assert_eq!(
            sanitize_session_name("my-repo", "feature/login"),
            "ok-my-repo-feature-login"
        );
        assert_eq!(
            sanitize_session_name("my repo", "feat ure"),
            "ok-my-repo-feat-ure"
        );
        assert_eq!(
            sanitize_session_name("repo", "feture-branch"),
            "ok-repo-feture-branch"
        );
        assert_eq!(
            sanitize_session_name("repo", "unicode-branch-\u{6587}\u{5b57}"),
            "ok-repo-unicode-branch---"
        );

        let long_repo = "r".repeat(220);
        let long_branch = "b".repeat(220);
        let result = sanitize_session_name(&long_repo, &long_branch);
        assert!(result.len() <= 200);
        assert!(result.starts_with("ok-"));
    }

    #[test]
    fn test_sanitize_session_name_for_project() {
        assert_eq!(
            sanitize_session_name_for_project(None, "my-repo", "feature/login"),
            "ok-my-repo-feature-login"
        );
        assert_eq!(
            sanitize_session_name_for_project(Some(" "), "my-repo", "feature/login"),
            "ok-my-repo-feature-login"
        );
        assert_eq!(
            sanitize_session_name_for_project(Some("my-project"), "my-repo", "feature/login"),
            "ok-my-project-my-repo-feature-login"
        );
        assert_eq!(
            sanitize_session_name_for_project(Some("my project"), "my-repo", "feature/login"),
            "ok-my-project-my-repo-feature-login"
        );
    }

    #[test]
    fn test_socket_args_builder() {
        assert_eq!(socket_args("sock"), vec!["-L", "sock"]);
    }

    #[test]
    fn test_has_session_args_builder() {
        assert_eq!(
            has_session_args("ok-test"),
            vec!["has-session", "-t", "ok-test"]
        );
    }

    #[test]
    fn test_new_session_args_builder() {
        let args = new_session_args("ok-test", Path::new("/tmp/worktree"));
        assert_eq!(
            args,
            vec!["new-session", "-d", "-s", "ok-test", "-c", "/tmp/worktree"]
        );
    }

    #[test]
    fn test_kill_session_args_builder() {
        assert_eq!(
            kill_session_args("ok-test"),
            vec!["kill-session", "-t", "ok-test"]
        );
    }

    #[test]
    fn test_switch_client_args_builder() {
        assert_eq!(
            switch_client_args("ok-target"),
            vec!["switch-client", "-t", "ok-target"]
        );
    }

    #[test]
    fn test_return_binding_args_builder() {
        assert_eq!(
            return_binding_args(),
            vec!["bind-key", "-T", "prefix", "K", "switch-client", "-l"]
        );
    }

    #[test]
    fn test_list_sessions_args_builder() {
        assert_eq!(
            list_sessions_args(),
            vec![
                "list-sessions",
                "-F",
                "#{session_name}\t#{session_created}\t#{session_attached}"
            ]
        );
    }

    #[test]
    fn test_capture_pane_window_args_builder() {
        assert_eq!(
            capture_pane_window_args("ok-test", "-50"),
            vec!["capture-pane", "-t", "ok-test:0.0", "-p", "-S", "-50"]
        );
    }

    #[test]
    fn test_capture_pane_session_args_builder() {
        assert_eq!(
            capture_pane_session_args("ok-test", "-50"),
            vec!["capture-pane", "-t", "ok-test", "-p", "-S", "-50"]
        );
    }

    #[test]
    fn test_list_panes_args_builder() {
        assert_eq!(
            list_panes_args("ok-test"),
            vec!["list-panes", "-t", "ok-test", "-F", "#{pane_pid}"]
        );
    }

    #[test]
    fn test_tmux_create_session() {
        if !tmux_available() {
            return;
        }
        let session_name = unique_session_name("create");
        let _cleanup = SessionCleanup::new(session_name.clone());

        tmux_create_session(
            &session_name,
            Path::new("."),
            Some("printf 'hello-from-create-test\\n'; sleep 2"),
        )
        .expect("create session should succeed");

        assert!(wait_for_session(&session_name, Duration::from_secs(2)));

        let sessions = tmux_list_sessions();
        assert!(sessions.iter().any(|session| session.name == session_name));

        let pane_pid = wait_for_pane_pid(&session_name, Duration::from_secs(2));
        assert!(pane_pid.is_some());

        if tmux_session_exists(&session_name) {
            let _ = tmux_kill_session(&session_name);
        }
        assert!(!tmux_session_exists(&session_name));
    }

    #[test]
    fn test_tmux_capture_pane() {
        if !tmux_available() {
            return;
        }
        let session_name = unique_session_name("capture");
        let _cleanup = SessionCleanup::new(session_name.clone());

        tmux_create_session(
            &session_name,
            Path::new("."),
            Some("printf 'line-one\\nline-two\\n'; sleep 2"),
        )
        .expect("create session should succeed");

        assert!(wait_for_session(&session_name, Duration::from_secs(2)));
        let captured = tmux_capture_pane(&session_name, 20).expect("capture should succeed");
        assert!(captured.contains("line-one"));
        assert!(captured.contains("line-two"));

        if tmux_session_exists(&session_name) {
            let _ = tmux_kill_session(&session_name);
        }
    }

    fn tmux_available() -> bool {
        Command::new("tmux")
            .arg("-V")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    fn unique_session_name(prefix: &str) -> String {
        let micros = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be valid")
            .as_micros();
        format!("tmux-{prefix}-{micros}")
    }
}
