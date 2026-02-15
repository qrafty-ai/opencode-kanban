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
        .args(["has-session", "-t", session_name])
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

pub fn tmux_create_session(
    session_name: &str,
    working_dir: &Path,
    command: Option<&str>,
) -> Result<()> {
    let output = tmux_command()
        .args([
            "new-session",
            "-d",
            "-s",
            session_name,
            "-c",
            &working_dir.to_string_lossy(),
        ])
        .output()
        .context("failed to run tmux new-session")?;

    ensure_success(&output, "new-session")?;

    if let Some(command) = command {
        tmux_send_keys(session_name, command)?;
    }

    Ok(())
}

pub fn tmux_send_keys(session_name: &str, command: &str) -> Result<()> {
    let output = tmux_command()
        .args(["send-keys", "-t", session_name, command, "Enter"])
        .output()
        .context("failed to run tmux send-keys")?;
    ensure_success(&output, "send-keys")
}

pub fn tmux_kill_session(session_name: &str) -> Result<()> {
    let output = tmux_command()
        .args(["kill-session", "-t", session_name])
        .output()
        .context("failed to run tmux kill-session")?;
    ensure_success(&output, "kill-session")
}

pub fn tmux_switch_client(session_name: &str) -> Result<()> {
    // Try to get current client from default socket (not our dedicated socket)
    // because the kanban runs in the user's regular tmux
    let current_client = get_current_client_from_default_socket();
    let output = if let Some(client) = current_client {
        // Switch using the client we found
        Command::new("tmux")
            .args(["switch-client", "-c", &client, "-t", session_name])
            .output()
            .context("failed to run tmux switch-client")?
    } else {
        // Fallback: try direct switch (will fail if no current client)
        Command::new("tmux")
            .args(["switch-client", "-t", session_name])
            .output()
            .context("failed to run tmux switch-client")?
    };
    ensure_success_with_output(&output, "switch-client")
}

fn get_current_client_from_default_socket() -> Option<String> {
    let output = Command::new("tmux")
        .args(["display-message", "-p", "#{client_name}"])
        .output()
        .ok()?;
    if output.status.success() {
        let client = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if client.is_empty() {
            None
        } else {
            Some(client)
        }
    } else {
        None
    }
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
    let output = tmux_command()
        .args([
            "list-sessions",
            "-F",
            "#{session_name}\t#{session_created}\t#{session_attached}",
        ])
        .output();

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
    let target = format!("{session_name}:0.0");
    let output = tmux_command()
        .args(["capture-pane", "-t", &target, "-p", "-S", &start])
        .output()
        .context("failed to run tmux capture-pane")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("can't find window: 0") {
            let retry = tmux_command()
                .args(["capture-pane", "-t", session_name, "-p", "-S", &start])
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
        .args(["list-panes", "-t", session_name, "-F", "#{pane_pid}"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout.lines().next()?.trim().parse().ok()
}

pub fn sanitize_session_name(repo_name: &str, branch_name: &str) -> String {
    let repo = sanitize_fragment(repo_name);
    let branch = sanitize_fragment(branch_name);
    let mut session_name = format!("ok-{repo}-{branch}");
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
    cmd.args(["-L", socket.as_str()]);
    cmd
}

fn tmux_socket() -> String {
    if let Ok(socket) = env::var("OPENCODE_KANBAN_TMUX_SOCKET") {
        let trimmed = socket.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }

    if cfg!(test) {
        "opencode-kanban-test".to_string()
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
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

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
    fn test_tmux_create_session() {
        if !tmux_available() {
            return;
        }

        cleanup_test_server();
        let session_name = unique_session_name("create");

        tmux_create_session(
            &session_name,
            Path::new("."),
            Some("printf 'hello-from-create-test\\n'; sleep 2"),
        )
        .expect("create session should succeed");

        thread::sleep(Duration::from_millis(200));
        assert!(tmux_session_exists(&session_name));

        let sessions = tmux_list_sessions();
        assert!(sessions.iter().any(|session| session.name == session_name));

        let pane_pid = tmux_get_pane_pid(&session_name);
        assert!(pane_pid.is_some());

        if tmux_session_exists(&session_name) {
            let _ = tmux_kill_session(&session_name);
        }
        assert!(!tmux_session_exists(&session_name));

        cleanup_test_server();
    }

    #[test]
    fn test_tmux_capture_pane() {
        if !tmux_available() {
            return;
        }

        cleanup_test_server();
        let session_name = unique_session_name("capture");

        tmux_create_session(
            &session_name,
            Path::new("."),
            Some("printf 'line-one\\nline-two\\n'; sleep 2"),
        )
        .expect("create session should succeed");

        thread::sleep(Duration::from_millis(200));
        let captured = tmux_capture_pane(&session_name, 20).expect("capture should succeed");
        assert!(captured.contains("line-one"));
        assert!(captured.contains("line-two"));

        if tmux_session_exists(&session_name) {
            let _ = tmux_kill_session(&session_name);
        }
        cleanup_test_server();
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

    fn cleanup_test_server() {
        let socket = tmux_socket();
        let _ = Command::new("tmux")
            .args(["-L", socket.as_str(), "kill-server"])
            .output();
    }
}
