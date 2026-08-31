use crate::app::ErrorDialogState;

pub(crate) fn create_task_error_dialog_state(err: &anyhow::Error) -> ErrorDialogState {
    let detail = format!("{err:#}");

    let title = if detail.contains("worktree creation failed") {
        "Worktree creation failed".to_string()
    } else if detail.contains("tmux session creation failed") {
        "Tmux session failed".to_string()
    } else {
        "Task creation failed".to_string()
    };

    ErrorDialogState { title, detail }
}
