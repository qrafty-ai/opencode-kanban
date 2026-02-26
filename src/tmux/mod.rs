#![allow(dead_code)]

use std::env;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, anyhow, bail};
use termlauncher::{Application, CustomTerminal, Error as TermlauncherError, Terminal};

const TMUX_SOCKET: &str = "";

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct PopupThemeStyle {
    pub popup_style: String,
    pub border_style: String,
}

impl PopupThemeStyle {
    pub fn plain() -> Self {
        Self {
            popup_style: "fg=default,bg=default".to_string(),
            border_style: "fg=default,bg=default".to_string(),
        }
    }
}

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

pub fn tmux_switch_client(
    session_name: &str,
    reopen_lines: &[String],
    style: &PopupThemeStyle,
) -> Result<()> {
    tmux_ensure_return_binding()?;
    tmux_ensure_overlay_binding(reopen_lines, style)?;

    let output = Command::new("tmux")
        .args(switch_client_args(session_name))
        .output()
        .context("failed to run tmux switch-client")?;
    ensure_success_with_output(&output, "switch-client")
}

pub fn tmux_open_session_in_new_terminal(
    session_name: &str,
    working_dir: &Path,
    terminal_executable: Option<&str>,
    terminal_launch_args: &[String],
) -> Result<()> {
    let mut app = Application::new("tmux").with_working_dir(working_dir);
    for arg in tmux_attach_args(session_name) {
        app = app.with_arg(&arg);
    }

    let launch_result = if let Some(executable) = normalize_terminal_executable(terminal_executable)
    {
        let custom_terminal = Terminal::Custom(CustomTerminal {
            executable,
            arguments: normalize_terminal_launch_args(terminal_launch_args),
            ..CustomTerminal::default()
        });
        app.launch_with(&custom_terminal)
    } else {
        app.launch()
    };

    match launch_result {
        Ok(_) => Ok(()),
        Err(err) => Err(anyhow!(terminal_launch_error_message(
            &err,
            terminal_executable
        ))),
    }
}

pub fn tmux_show_popup(lines: &[String], style: &PopupThemeStyle) -> Result<()> {
    let command = popup_shell_command(lines);
    let output = tmux_command()
        .args(display_popup_args(&command, style))
        .output()
        .context("failed to run tmux display-popup")?;
    ensure_success_with_output(&output, "display-popup")
}

fn tmux_ensure_return_binding() -> Result<()> {
    let output = Command::new("tmux")
        .args(return_binding_args())
        .output()
        .context("failed to configure return key binding")?;
    ensure_success_with_output(&output, "bind-key")
}

fn tmux_ensure_overlay_binding(lines: &[String], style: &PopupThemeStyle) -> Result<()> {
    let output = Command::new("tmux")
        .args(overlay_binding_args(lines, style))
        .output()
        .context("failed to configure attach overlay key binding")?;
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

pub fn tmux_display_message(
    session_name: &str,
    message: &str,
    display_duration_ms: u64,
) -> Result<()> {
    let clients = tmux_list_clients_for_session(session_name);

    if clients.is_empty() {
        let output = tmux_command()
            .args(display_message_args(
                session_name,
                message,
                display_duration_ms,
            ))
            .output()
            .context("failed to run tmux display-message")?;
        return ensure_success_with_output(&output, "display-message");
    }

    for client_id in clients {
        let output = tmux_command()
            .args(display_message_to_client_args(
                &client_id,
                message,
                display_duration_ms,
            ))
            .output()
            .context("failed to run tmux display-message")?;
        let _ = ensure_success_with_output(&output, "display-message");
    }

    Ok(())
}

pub fn tmux_list_project_sessions(project_slug: &str) -> Vec<String> {
    let prefix = format!("ok-{project_slug}-");
    let kanban_session = "opencode-kanban";

    tmux_list_sessions()
        .into_iter()
        .filter(|s| s.name == kanban_session || s.name.starts_with(&prefix))
        .map(|s| s.name)
        .collect()
}

pub fn tmux_broadcast_to_sessions(
    session_names: &[String],
    message: &str,
    display_duration_ms: u64,
) -> Result<()> {
    for session_name in session_names {
        let _ = tmux_display_message(session_name, message, display_duration_ms);
    }
    Ok(())
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

fn display_message_args(
    session_name: &str,
    message: &str,
    display_duration_ms: u64,
) -> Vec<String> {
    vec![
        "display-message".to_string(),
        "-t".to_string(),
        format!("{session_name}:."),
        "-d".to_string(),
        display_duration_ms.to_string(),
        message.to_string(),
    ]
}

fn tmux_list_clients_for_session(session_name: &str) -> Vec<String> {
    let output = match tmux_command()
        .args(list_clients_args(session_name))
        .output()
    {
        Ok(output) => output,
        Err(_) => return Vec::new(),
    };

    if !output.status.success() {
        return Vec::new();
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect()
}

fn list_clients_args(session_name: &str) -> Vec<String> {
    vec![
        "list-clients".to_string(),
        "-t".to_string(),
        session_name.to_string(),
        "-F".to_string(),
        "#{client_id}".to_string(),
    ]
}

fn display_message_to_client_args(
    client_id: &str,
    message: &str,
    display_duration_ms: u64,
) -> Vec<String> {
    vec![
        "display-message".to_string(),
        "-c".to_string(),
        client_id.to_string(),
        "-d".to_string(),
        display_duration_ms.to_string(),
        message.to_string(),
    ]
}

fn switch_client_args(session_name: &str) -> Vec<String> {
    vec![
        "switch-client".to_string(),
        "-t".to_string(),
        session_name.to_string(),
    ]
}

fn tmux_attach_args(session_name: &str) -> Vec<String> {
    let mut args = Vec::new();
    let socket = tmux_socket();
    if !socket.is_empty() {
        args.push("-L".to_string());
        args.push(socket);
    }

    args.push("attach".to_string());
    args.push("-t".to_string());
    args.push(session_name.to_string());
    args
}

fn normalize_terminal_executable(terminal_executable: Option<&str>) -> Option<String> {
    terminal_executable
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn normalize_terminal_launch_args(terminal_launch_args: &[String]) -> Vec<String> {
    terminal_launch_args
        .iter()
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect()
}

fn terminal_launch_error_message(
    error: &TermlauncherError,
    terminal_executable: Option<&str>,
) -> String {
    match error {
        TermlauncherError::NoSupportedTerminalAvailable => {
            "Failed to open task in a new terminal: no supported terminal was detected. Install a supported terminal or set settings.terminal_executable.".to_string()
        }
        TermlauncherError::TerminalNotFound(name) => {
            if let Some(configured) = normalize_terminal_executable(terminal_executable) {
                format!(
                    "Failed to open task in a new terminal: configured terminal executable '{}' was not found on PATH (resolved as '{}').",
                    configured, name
                )
            } else {
                format!(
                    "Failed to open task in a new terminal: terminal executable '{}' was not found on PATH.",
                    name
                )
            }
        }
        TermlauncherError::IOError(io_error) => format!(
            "Failed to open task in a new terminal: unable to launch terminal process ({io_error})."
        ),
    }
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

fn overlay_binding_args(lines: &[String], style: &PopupThemeStyle) -> Vec<String> {
    let command = popup_shell_command(lines);
    vec![
        "bind-key".to_string(),
        "-T".to_string(),
        "prefix".to_string(),
        "O".to_string(),
        "display-popup".to_string(),
        "-E".to_string(),
        "-s".to_string(),
        style.popup_style.clone(),
        "-S".to_string(),
        style.border_style.clone(),
        "-w".to_string(),
        "76%".to_string(),
        "-h".to_string(),
        "64%".to_string(),
        "-x".to_string(),
        "C".to_string(),
        "-y".to_string(),
        "C".to_string(),
        command,
    ]
}

fn display_popup_args(command: &str, style: &PopupThemeStyle) -> Vec<String> {
    vec![
        "display-popup".to_string(),
        "-E".to_string(),
        "-s".to_string(),
        style.popup_style.clone(),
        "-S".to_string(),
        style.border_style.clone(),
        "-w".to_string(),
        "76%".to_string(),
        "-h".to_string(),
        "64%".to_string(),
        "-x".to_string(),
        "C".to_string(),
        "-y".to_string(),
        "C".to_string(),
        command.to_string(),
    ]
}

fn popup_shell_command(lines: &[String]) -> String {
    let mut command = String::from("printf '%s\\n'");
    for line in lines {
        command.push(' ');
        command.push_str(&shell_single_quote(line));
    }
    command.push_str("; printf '%s' 'Press Enter to close'; read -r _");
    command
}

fn shell_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn list_sessions_args() -> Vec<String> {
    vec![
        "list-sessions".to_string(),
        "-F".to_string(),
        "#{session_name}\t#{session_created}\t#{session_attached}".to_string(),
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
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

    fn sleep_for(duration: Duration) {
        if let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
        {
            runtime.block_on(tokio::time::sleep(duration));
        }
    }

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
            sleep_for(Duration::from_millis(50));
        }
        false
    }

    fn wait_for_pane_pid(session_name: &str, timeout: Duration) -> Option<u32> {
        let start = Instant::now();
        while start.elapsed() < timeout {
            if let Some(pid) = tmux_get_pane_pid(session_name) {
                return Some(pid);
            }
            sleep_for(Duration::from_millis(50));
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
    fn test_display_message_args_builder() {
        assert_eq!(
            display_message_args("ok-test", "done", 3_000),
            vec!["display-message", "-t", "ok-test:.", "-d", "3000", "done"]
        );
    }

    #[test]
    fn test_display_message_to_client_args_builder() {
        assert_eq!(
            display_message_to_client_args("%1", "done", 3_000),
            vec!["display-message", "-c", "%1", "-d", "3000", "done"]
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
    fn test_tmux_attach_args_builder() {
        let args = tmux_attach_args("ok-target");
        assert!(args.ends_with(&[
            "attach".to_string(),
            "-t".to_string(),
            "ok-target".to_string()
        ]));
        let socket = tmux_socket();
        if !socket.is_empty() {
            assert_eq!(args[0], "-L");
            assert_eq!(args[1], socket);
        }
    }

    #[test]
    fn test_return_binding_args_builder() {
        assert_eq!(
            return_binding_args(),
            vec!["bind-key", "-T", "prefix", "K", "switch-client", "-l"]
        );
    }

    #[test]
    fn test_overlay_binding_args_builder() {
        let style = PopupThemeStyle {
            popup_style: "fg=#ffffff,bg=#101010".to_string(),
            border_style: "fg=#5fa8ff,bg=#101010".to_string(),
        };
        let lines = vec![
            "Task helper".to_string(),
            "Prefix+K  return to kanban".to_string(),
        ];
        assert_eq!(
            overlay_binding_args(&lines, &style),
            vec![
                "bind-key",
                "-T",
                "prefix",
                "O",
                "display-popup",
                "-E",
                "-s",
                "fg=#ffffff,bg=#101010",
                "-S",
                "fg=#5fa8ff,bg=#101010",
                "-w",
                "76%",
                "-h",
                "64%",
                "-x",
                "C",
                "-y",
                "C",
                "printf '%s\\n' 'Task helper' 'Prefix+K  return to kanban'; printf '%s' 'Press Enter to close'; read -r _",
            ]
        );
    }

    #[test]
    fn test_display_popup_args_builder() {
        let style = PopupThemeStyle {
            popup_style: "fg=#ffffff,bg=#101010".to_string(),
            border_style: "fg=#5fa8ff,bg=#101010".to_string(),
        };
        assert_eq!(
            display_popup_args("printf 'hello'", &style),
            vec![
                "display-popup",
                "-E",
                "-s",
                "fg=#ffffff,bg=#101010",
                "-S",
                "fg=#5fa8ff,bg=#101010",
                "-w",
                "76%",
                "-h",
                "64%",
                "-x",
                "C",
                "-y",
                "C",
                "printf 'hello'",
            ]
        );
    }

    #[test]
    fn test_popup_theme_style_plain_defaults() {
        assert_eq!(
            PopupThemeStyle::plain(),
            PopupThemeStyle {
                popup_style: "fg=default,bg=default".to_string(),
                border_style: "fg=default,bg=default".to_string(),
            }
        );
    }

    #[test]
    fn test_popup_shell_command_escapes_lines() {
        let command = popup_shell_command(&["can't fail".to_string(), "line two".to_string()]);
        assert_eq!(
            command,
            "printf '%s\\n' 'can'\\''t fail' 'line two'; printf '%s' 'Press Enter to close'; read -r _"
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
