use crate::app::ErrorDialogState;

pub(crate) fn create_task_error_dialog_state(err: &anyhow::Error) -> ErrorDialogState {
    let detail = format!("{err:#}");

    if let Some(branch) = parse_existing_branch_name(&detail) {
        return ErrorDialogState {
            title: "Branch already exists".to_string(),
            detail: format!(
                "Branch `{branch}` already exists in this repository, so a new worktree branch cannot be created.\n\nChoose a different branch name, or delete/rename the existing local branch and try again."
            ),
        };
    }

    let title = if detail.contains("worktree creation failed") {
        "Worktree creation failed".to_string()
    } else if detail.contains("tmux session creation failed") {
        "Tmux session failed".to_string()
    } else {
        "Task creation failed".to_string()
    };

    ErrorDialogState { title, detail }
}

pub(crate) fn parse_existing_branch_name(detail: &str) -> Option<String> {
    detail.lines().find_map(|line| {
        let trimmed = line.trim();
        let rest = trimmed.strip_prefix("fatal: a branch named '")?;
        let (branch_name, _) = rest.split_once("' already exists")?;
        if branch_name.is_empty() {
            None
        } else {
            Some(branch_name.to_string())
        }
    })
}
