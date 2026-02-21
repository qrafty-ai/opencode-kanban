mod attach;
mod create_task;
mod errors;
mod recovery;

pub(crate) use attach::attach_task_with_runtime;
#[cfg(test)]
pub(crate) use attach::{build_attach_popup_lines, popup_style_from_theme, tmux_hex_color};
pub(crate) use create_task::{
    create_task_pipeline_with_runtime, rank_repos_for_query, repo_selection_usage_map,
};
#[cfg(test)]
pub(crate) use create_task::{
    repo_match_candidates, repo_selection_command_id, resolve_repo_for_creation,
};
pub(crate) use errors::create_task_error_dialog_state;
#[cfg(test)]
pub(crate) use errors::parse_existing_branch_name;
pub(crate) use recovery::reconcile_startup_tasks;
