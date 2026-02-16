//! Task and category action operations

use std::collections::HashMap;
use std::path::Path;

use anyhow::Result;
use uuid::Uuid;

use crate::db::Database;
use crate::git::{git_delete_branch, git_remove_worktree};
use crate::tmux::tmux_kill_session;
use crate::types::{Category, Repo, Task};

use super::state::{ActiveDialog, ErrorDialogState};

/// Move selected task to the left column
pub fn move_task_left(
    db: &Database,
    tasks: &mut [Task],
    categories: &[Category],
    focused_column: &mut usize,
    selected_task_per_column: &mut HashMap<usize, usize>,
) -> Result<()> {
    if *focused_column == 0 {
        return Ok(());
    }
    let Some(category) = categories.get(*focused_column) else {
        return Ok(());
    };
    let task = tasks.iter().find(|t| t.category_id == category.id).cloned();
    let Some(task) = task else {
        return Ok(());
    };
    let target_column = *focused_column - 1;
    let target_category = &categories[target_column];
    db.update_task_category(task.id, target_category.id, 0)?;
    *focused_column = target_column;
    selected_task_per_column.insert(target_column, 0);
    Ok(())
}

/// Move selected task to the right column
pub fn move_task_right(
    db: &Database,
    tasks: &mut [Task],
    categories: &[Category],
    focused_column: &mut usize,
    selected_task_per_column: &mut HashMap<usize, usize>,
) -> Result<()> {
    if *focused_column >= categories.len() - 1 {
        return Ok(());
    }
    let Some(category) = categories.get(*focused_column) else {
        return Ok(());
    };
    let task = tasks.iter().find(|t| t.category_id == category.id).cloned();
    let Some(task) = task else {
        return Ok(());
    };
    let target_column = *focused_column + 1;
    let target_category = &categories[target_column];
    db.update_task_category(task.id, target_category.id, 0)?;
    *focused_column = target_column;
    selected_task_per_column.insert(target_column, 0);
    Ok(())
}

/// Move selected task up in its column
pub fn move_task_up(
    db: &Database,
    tasks: &mut [Task],
    categories: &[Category],
    focused_column: usize,
    selected_task_per_column: &mut HashMap<usize, usize>,
) -> Result<()> {
    let Some(category) = categories.get(focused_column) else {
        return Ok(());
    };
    let mut column_tasks: Vec<_> = tasks
        .iter()
        .filter(|t| t.category_id == category.id)
        .cloned()
        .collect();
    column_tasks.sort_by_key(|t| t.position);
    if column_tasks.len() < 2 {
        return Ok(());
    }
    let selected = selected_task_per_column
        .get(&focused_column)
        .copied()
        .unwrap_or(0)
        .min(column_tasks.len() - 1);
    if selected == 0 {
        return Ok(());
    }
    column_tasks.swap(selected - 1, selected);
    for (idx, task) in column_tasks.iter().enumerate() {
        db.update_task_position(task.id, idx as i64)?;
    }
    selected_task_per_column.insert(focused_column, selected - 1);
    Ok(())
}

/// Move selected task down in its column
pub fn move_task_down(
    db: &Database,
    tasks: &mut [Task],
    categories: &[Category],
    focused_column: usize,
    selected_task_per_column: &mut HashMap<usize, usize>,
) -> Result<()> {
    let Some(category) = categories.get(focused_column) else {
        return Ok(());
    };
    let mut column_tasks: Vec<_> = tasks
        .iter()
        .filter(|t| t.category_id == category.id)
        .cloned()
        .collect();
    column_tasks.sort_by_key(|t| t.position);
    if column_tasks.len() < 2 {
        return Ok(());
    }
    let selected = selected_task_per_column
        .get(&focused_column)
        .copied()
        .unwrap_or(0)
        .min(column_tasks.len() - 1);
    if selected + 1 >= column_tasks.len() {
        return Ok(());
    }
    column_tasks.swap(selected, selected + 1);
    for (idx, task) in column_tasks.iter().enumerate() {
        db.update_task_position(task.id, idx as i64)?;
    }
    selected_task_per_column.insert(focused_column, selected + 1);
    Ok(())
}

/// Confirm category creation or rename
pub fn confirm_category_input(
    db: &Database,
    dialog: &mut ActiveDialog,
    categories: &mut Vec<Category>,
    focused_column: &mut usize,
    selected_task_per_column: &mut HashMap<usize, usize>,
) -> Result<()> {
    let state = match dialog {
        ActiveDialog::CategoryInput(state) => state.clone(),
        _ => return Ok(()),
    };

    let name = state.name_input.trim();
    if name.is_empty() {
        *dialog = ActiveDialog::Error(ErrorDialogState {
            title: "Invalid category".to_string(),
            detail: "Category name cannot be empty.".to_string(),
        });
        return Ok(());
    }

    match state.mode {
        super::state::CategoryInputMode::Add => {
            let next_position = categories
                .iter()
                .map(|category| category.position)
                .max()
                .unwrap_or(-1)
                + 1;
            let created = db.add_category(name, next_position, None)?;
            *dialog = ActiveDialog::None;
            categories.push(created.clone());
            if let Some(index) = categories.iter().position(|c| c.id == created.id) {
                *focused_column = index;
                selected_task_per_column.entry(index).or_insert(0);
            }
        }
        super::state::CategoryInputMode::Rename => {
            let Some(category_id) = state.category_id else {
                return Ok(());
            };
            db.rename_category(category_id, name)?;
            *dialog = ActiveDialog::None;
            if let Some(cat) = categories.iter_mut().find(|c| c.id == category_id) {
                cat.name = name.to_string();
            }
        }
    }

    Ok(())
}

/// Confirm category deletion
pub fn confirm_delete_category(
    db: &Database,
    dialog: &ActiveDialog,
) -> Result<Result<(), ErrorDialogState>> {
    let state = match dialog {
        ActiveDialog::DeleteCategory(state) => state.clone(),
        _ => return Ok(Ok(())),
    };

    if state.task_count > 0 {
        return Ok(Err(ErrorDialogState {
            title: "Category not empty".to_string(),
            detail: format!(
                "Cannot delete '{}' because it still contains {} task(s).",
                state.category_name, state.task_count
            ),
        }));
    }

    db.delete_category(state.category_id)?;
    Ok(Ok(()))
}

/// Confirm task deletion with cleanup
pub fn confirm_delete_task(
    db: &Database,
    tasks: &[Task],
    repos: &[Repo],
    dialog: &ActiveDialog,
) -> Result<Option<Uuid>> {
    let state = match dialog {
        ActiveDialog::DeleteTask(state) => state.clone(),
        _ => return Ok(None),
    };

    let task = tasks.iter().find(|t| t.id == state.task_id);
    let Some(task) = task else {
        return Ok(Some(state.task_id));
    };

    let repo = repos.iter().find(|repo| repo.id == task.repo_id);

    if state.kill_tmux
        && let Some(ref session_name) = task.tmux_session_name
    {
        let _ = tmux_kill_session(session_name);
    }

    if state.remove_worktree
        && let (Some(worktree_path), Some(r)) = (&task.worktree_path, repo.as_ref())
    {
        let worktree = Path::new(worktree_path);
        let repo_path = Path::new(&r.path);
        if worktree.exists() {
            let _ = git_remove_worktree(repo_path, worktree);
        }
    }

    if state.delete_branch
        && let (Some(r), true) = (repo, !task.branch.is_empty())
    {
        let _ = git_delete_branch(Path::new(&r.path), &task.branch);
    }

    db.delete_task(state.task_id)?;
    Ok(Some(state.task_id))
}
