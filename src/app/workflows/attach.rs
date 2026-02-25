use std::path::{Path, PathBuf};

use anyhow::Result;
use tracing::warn;
use tuirealm::ratatui::style::Color;
use uuid::Uuid;

use crate::app::runtime::{RecoveryRuntime, next_available_session_name};
use crate::app::state::AttachTaskResult;
use crate::db::Database;
use crate::opencode::{Status, opencode_attach_command};
use crate::theme::Theme;
use crate::tmux::PopupThemeStyle;
use crate::types::{Repo, SessionTodoItem, Task};

pub(crate) fn attach_task_with_runtime(
    db: &Database,
    project_slug: Option<&str>,
    task: &Task,
    repo: &Repo,
    task_todos: &[SessionTodoItem],
    theme: &Theme,
    runtime: &impl RecoveryRuntime,
) -> Result<AttachTaskResult> {
    let ensured = match ensure_task_session_with_runtime(db, project_slug, task, repo, runtime)? {
        EnsureTaskSessionOutcome::Ready(ensured) => ensured,
        EnsureTaskSessionOutcome::Early(result) => return Ok(result),
    };
    let session_name = ensured.session_name;

    let popup_style = popup_style_from_theme(theme);
    let popup_lines = build_attach_popup_lines(task, repo, &session_name, task_todos);
    runtime.switch_client(&session_name, &popup_lines, &popup_style)?;
    maybe_show_attach_popup(
        db,
        task.id,
        &session_name,
        task.attach_overlay_shown,
        &popup_lines,
        &popup_style,
        runtime,
    );
    Ok(AttachTaskResult::Attached)
}

pub(crate) fn open_task_in_new_terminal_with_runtime(
    db: &Database,
    project_slug: Option<&str>,
    task: &Task,
    repo: &Repo,
    terminal_executable: Option<&str>,
    terminal_launch_args: &[String],
    runtime: &impl RecoveryRuntime,
) -> Result<AttachTaskResult> {
    let ensured = match ensure_task_session_with_runtime(db, project_slug, task, repo, runtime)? {
        EnsureTaskSessionOutcome::Ready(ensured) => ensured,
        EnsureTaskSessionOutcome::Early(result) => return Ok(result),
    };
    runtime.open_in_new_terminal(
        &ensured.session_name,
        &ensured.working_dir,
        terminal_executable,
        terminal_launch_args,
    )?;
    Ok(AttachTaskResult::Attached)
}

struct EnsureTaskSessionResult {
    session_name: String,
    working_dir: PathBuf,
}

enum EnsureTaskSessionOutcome {
    Ready(EnsureTaskSessionResult),
    Early(AttachTaskResult),
}

fn ensure_task_session_with_runtime(
    db: &Database,
    project_slug: Option<&str>,
    task: &Task,
    repo: &Repo,
    runtime: &impl RecoveryRuntime,
) -> Result<EnsureTaskSessionOutcome> {
    if !runtime.repo_exists(Path::new(&repo.path)) {
        db.update_task_status(task.id, Status::Idle.as_str())?;
        return Ok(EnsureTaskSessionOutcome::Early(
            AttachTaskResult::RepoUnavailable,
        ));
    }

    if let Some(session_name) = task.tmux_session_name.as_deref()
        && runtime.session_exists(session_name)
    {
        let working_dir = task
            .worktree_path
            .as_deref()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(&repo.path));
        return Ok(EnsureTaskSessionOutcome::Ready(EnsureTaskSessionResult {
            session_name: session_name.to_string(),
            working_dir,
        }));
    }

    let Some(worktree_path_str) = task.worktree_path.as_deref() else {
        return Ok(EnsureTaskSessionOutcome::Early(
            AttachTaskResult::WorktreeNotFound,
        ));
    };
    let worktree_path = Path::new(worktree_path_str);
    if !runtime.worktree_exists(worktree_path) {
        return Ok(EnsureTaskSessionOutcome::Early(
            AttachTaskResult::WorktreeNotFound,
        ));
    }

    let session_name = next_available_session_name(
        task.tmux_session_name.as_deref(),
        project_slug,
        &repo.name,
        &task.branch,
        runtime,
    );

    let command = opencode_attach_command(
        task.opencode_session_id.as_deref(),
        task.worktree_path.as_deref(),
    );

    runtime.create_session(&session_name, worktree_path, &command)?;
    db.update_task_tmux(
        task.id,
        Some(session_name.clone()),
        task.worktree_path.clone(),
    )?;
    db.update_task_status(task.id, Status::Idle.as_str())?;

    Ok(EnsureTaskSessionOutcome::Ready(EnsureTaskSessionResult {
        session_name,
        working_dir: worktree_path.to_path_buf(),
    }))
}

fn maybe_show_attach_popup(
    db: &Database,
    task_id: Uuid,
    session_name: &str,
    attach_overlay_shown: bool,
    popup_lines: &[String],
    popup_style: &PopupThemeStyle,
    runtime: &impl RecoveryRuntime,
) {
    if attach_overlay_shown {
        return;
    }

    match runtime.show_attach_popup(popup_lines, popup_style) {
        Ok(()) => {
            if let Err(err) = db.update_task_attach_overlay_shown(task_id, true) {
                warn!(
                    error = %err,
                    task_id = %task_id,
                    session_name = %session_name,
                    "failed to persist attach popup shown state"
                );
            }
        }
        Err(err) => {
            warn!(
                error = %err,
                task_id = %task_id,
                session_name = %session_name,
                "failed to show attach popup overlay"
            );
        }
    }
}

pub(crate) fn popup_style_from_theme(theme: &Theme) -> PopupThemeStyle {
    let text = tmux_hex_color(theme.base.text);
    let surface = tmux_hex_color(theme.dialog.surface);
    let border = tmux_hex_color(theme.interactive.selected_border);
    PopupThemeStyle {
        popup_style: format!("fg={text},bg={surface}"),
        border_style: format!("fg={border},bg={surface}"),
    }
}

pub(crate) fn tmux_hex_color(color: Color) -> String {
    let (r, g, b) = match color {
        Color::Reset => (208, 208, 208),
        Color::Black => (0, 0, 0),
        Color::Red => (205, 49, 49),
        Color::Green => (13, 188, 121),
        Color::Yellow => (229, 229, 16),
        Color::Blue => (36, 114, 200),
        Color::Magenta => (188, 63, 188),
        Color::Cyan => (17, 168, 205),
        Color::Gray => (128, 128, 128),
        Color::DarkGray => (90, 90, 90),
        Color::LightRed => (241, 76, 76),
        Color::LightGreen => (35, 209, 139),
        Color::LightYellow => (245, 245, 67),
        Color::LightBlue => (59, 142, 234),
        Color::LightMagenta => (214, 112, 214),
        Color::LightCyan => (41, 184, 219),
        Color::White => (255, 255, 255),
        Color::Rgb(r, g, b) => (r, g, b),
        Color::Indexed(index) => {
            let value = index;
            (value, value, value)
        }
    };
    format!("#{r:02x}{g:02x}{b:02x}")
}

pub(crate) fn build_attach_popup_lines(
    task: &Task,
    repo: &Repo,
    session_name: &str,
    task_todos: &[SessionTodoItem],
) -> Vec<String> {
    let worktree = task.worktree_path.as_deref().unwrap_or("n/a");
    let mut lines = vec![
        "Task attached".to_string(),
        String::new(),
        format!("Title:   {}", task.title),
        format!("Repo:    {}", repo.name),
        format!("Branch:  {}", task.branch),
        format!("Session: {session_name}"),
        format!("Worktree:{worktree}"),
        String::new(),
        "Navigation".to_string(),
        "Prefix+K  return to kanban".to_string(),
        "Prefix+O  reopen helper".to_string(),
        "Prefix+d  detach from tmux".to_string(),
    ];

    lines.push(String::new());
    lines.push("Todo list".to_string());
    lines.extend(build_attach_popup_todo_lines(task_todos));
    lines.push(String::new());
    lines
}

fn build_attach_popup_todo_lines(task_todos: &[SessionTodoItem]) -> Vec<String> {
    if task_todos.is_empty() {
        return vec!["(no todos yet)".to_string()];
    }

    let active_index = task_todos.iter().position(|todo| !todo.completed);
    task_todos
        .iter()
        .enumerate()
        .map(|(index, todo)| {
            let marker = if todo.completed {
                "x"
            } else if Some(index) == active_index {
                ">"
            } else {
                " "
            };
            format!("[{marker}] {}", todo.content)
        })
        .collect()
}
