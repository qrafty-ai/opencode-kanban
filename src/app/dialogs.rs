//! Dialog handling logic for key events and dialog operations

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::types::{Category, Repo};

use super::messages::Message;
use super::state::{
    ActiveDialog, CategoryInputDialogState, CategoryInputField, DeleteCategoryDialogState,
    DeleteCategoryField, DeleteTaskDialogState, DeleteTaskField, NewProjectDialogState,
    NewProjectField, NewTaskDialogState, NewTaskField, WorktreeNotFoundDialogState,
    WorktreeNotFoundField,
};

/// Handle key events when a dialog is active
pub fn handle_dialog_key(
    dialog: &mut ActiveDialog,
    key: KeyEvent,
    _db: &crate::db::Database,
    repos: &mut [Repo],
    _categories: &mut [Category],
    _focused_column: &mut usize,
) -> Result<Option<Message>> {
    let mut follow_up: Option<Message> = None;

    match dialog {
        ActiveDialog::NewTask(state) => {
            handle_new_task_dialog_key(state, key, repos, &mut follow_up);
        }
        ActiveDialog::NewProject(state) => {
            handle_new_project_dialog_key(state, key, &mut follow_up);
        }
        ActiveDialog::CategoryInput(state) => {
            handle_category_input_dialog_key(state, key, &mut follow_up);
        }
        ActiveDialog::DeleteCategory(state) => {
            handle_delete_category_dialog_key(state, key, &mut follow_up);
        }
        ActiveDialog::DeleteTask(state) => {
            handle_delete_task_dialog_key(state, key, &mut follow_up);
        }
        ActiveDialog::WorktreeNotFound(state) => {
            handle_worktree_not_found_dialog_key(state, key, &mut follow_up);
        }
        ActiveDialog::RepoUnavailable(_) => {
            if matches!(key.code, KeyCode::Enter | KeyCode::Esc) {
                follow_up = Some(Message::RepoUnavailableDismiss);
            }
        }
        ActiveDialog::Help => {
            if matches!(key.code, KeyCode::Esc | KeyCode::Char('?')) {
                *dialog = ActiveDialog::None;
            }
        }
        ActiveDialog::Error(_) => {
            if matches!(key.code, KeyCode::Enter | KeyCode::Esc) {
                *dialog = ActiveDialog::None;
            }
        }
        ActiveDialog::CommandPalette(state) => match key.code {
            KeyCode::Esc => *dialog = ActiveDialog::None,
            KeyCode::Enter => {
                follow_up = state.selected_command_id().map(Message::ExecuteCommand);
            }
            KeyCode::Up => state.move_selection(-1),
            KeyCode::Down => state.move_selection(1),
            KeyCode::Char('k') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                state.move_selection(-1)
            }
            KeyCode::Char('j') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                state.move_selection(1)
            }
            KeyCode::Backspace => {
                if state.query.is_empty() {
                    *dialog = ActiveDialog::None;
                } else {
                    state.query.pop();
                    state.update_query();
                }
            }
            KeyCode::Char(ch)
                if !key.modifiers.contains(KeyModifiers::CONTROL)
                    && !key.modifiers.contains(KeyModifiers::ALT) =>
            {
                state.query.push(ch);
                state.update_query();
            }
            _ => {}
        },
        _ => {
            if key.code == KeyCode::Esc {
                *dialog = ActiveDialog::None;
            }
        }
    }

    Ok(follow_up)
}

fn handle_new_task_dialog_key(
    state: &mut NewTaskDialogState,
    key: KeyEvent,
    repos: &mut [Repo],
    follow_up: &mut Option<Message>,
) {
    let fields = [
        NewTaskField::Repo,
        NewTaskField::Branch,
        NewTaskField::Base,
        NewTaskField::Title,
        NewTaskField::EnsureBaseUpToDate,
        NewTaskField::Create,
        NewTaskField::Cancel,
    ];

    let mut focus_index = fields
        .iter()
        .position(|field| *field == state.focused_field)
        .unwrap_or(0);

    let move_focus = |current: usize, delta: isize| -> usize {
        let len = fields.len() as isize;
        let next = (current as isize + delta).rem_euclid(len);
        next as usize
    };

    match key.code {
        KeyCode::Esc => (),
        KeyCode::Tab | KeyCode::Down => {
            focus_index = move_focus(focus_index, 1);
            state.focused_field = fields[focus_index].clone();
        }
        KeyCode::BackTab | KeyCode::Up => {
            focus_index = move_focus(focus_index, -1);
            state.focused_field = fields[focus_index].clone();
        }
        KeyCode::Left if state.focused_field == NewTaskField::Repo => {
            if !repos.is_empty() {
                state.repo_input.clear();
                state.repo_idx = state.repo_idx.saturating_sub(1);
                if let Some(repo) = repos.get(state.repo_idx) {
                    state.base_input = repo_default_base(repo);
                }
            }
        }
        KeyCode::Right if state.focused_field == NewTaskField::Repo => {
            if !repos.is_empty() {
                state.repo_input.clear();
                state.repo_idx = (state.repo_idx + 1).min(repos.len() - 1);
                if let Some(repo) = repos.get(state.repo_idx) {
                    state.base_input = repo_default_base(repo);
                }
            }
        }
        KeyCode::Left if state.focused_field == NewTaskField::Create => {
            state.focused_field = NewTaskField::Cancel;
        }
        KeyCode::Right if state.focused_field == NewTaskField::Cancel => {
            state.focused_field = NewTaskField::Create;
        }
        KeyCode::Char(' ') | KeyCode::Enter
            if state.focused_field == NewTaskField::EnsureBaseUpToDate =>
        {
            state.ensure_base_up_to_date = !state.ensure_base_up_to_date;
        }
        KeyCode::Backspace => match state.focused_field {
            NewTaskField::Repo => {
                state.repo_input.pop();
            }
            NewTaskField::Branch => {
                state.branch_input.pop();
            }
            NewTaskField::Base => {
                state.base_input.pop();
            }
            NewTaskField::Title => {
                state.title_input.pop();
            }
            _ => {}
        },
        KeyCode::Enter => {
            *follow_up = Some(match state.focused_field {
                NewTaskField::Cancel => Message::DismissDialog,
                _ => Message::CreateTask,
            });
        }
        KeyCode::Char(ch) => match state.focused_field {
            NewTaskField::Repo => state.repo_input.push(ch),
            NewTaskField::Branch => state.branch_input.push(ch),
            NewTaskField::Base => state.base_input.push(ch),
            NewTaskField::Title => state.title_input.push(ch),
            _ => {}
        },
        _ => {}
    }
}

fn handle_new_project_dialog_key(
    state: &mut NewProjectDialogState,
    key: KeyEvent,
    follow_up: &mut Option<Message>,
) {
    let fields = [
        NewProjectField::Name,
        NewProjectField::Create,
        NewProjectField::Cancel,
    ];

    let mut focus_index = fields
        .iter()
        .position(|field| *field == state.focused_field)
        .unwrap_or(0);

    let move_focus = |current: usize, delta: isize| -> usize {
        let len = fields.len() as isize;
        let next = (current as isize + delta).rem_euclid(len);
        next as usize
    };

    match key.code {
        KeyCode::Esc => (),
        KeyCode::Tab | KeyCode::Down => {
            focus_index = move_focus(focus_index, 1);
            state.focused_field = fields[focus_index].clone();
        }
        KeyCode::BackTab | KeyCode::Up => {
            focus_index = move_focus(focus_index, -1);
            state.focused_field = fields[focus_index].clone();
        }
        KeyCode::Left if state.focused_field == NewProjectField::Create => {
            state.focused_field = NewProjectField::Cancel;
        }
        KeyCode::Right if state.focused_field == NewProjectField::Cancel => {
            state.focused_field = NewProjectField::Create;
        }
        KeyCode::Backspace => {
            if state.focused_field == NewProjectField::Name {
                state.name_input.pop();
            }
        }
        KeyCode::Enter => {
            *follow_up = Some(match state.focused_field {
                NewProjectField::Cancel => Message::DismissDialog,
                _ => Message::CreateProject,
            });
        }
        KeyCode::Char(ch) => {
            if state.focused_field == NewProjectField::Name {
                state.name_input.push(ch);
            }
        }
        _ => {}
    }
}

fn handle_category_input_dialog_key(
    state: &mut CategoryInputDialogState,
    key: KeyEvent,
    follow_up: &mut Option<Message>,
) {
    let fields = [
        CategoryInputField::Name,
        CategoryInputField::Confirm,
        CategoryInputField::Cancel,
    ];

    let mut focus_index = fields
        .iter()
        .position(|field| *field == state.focused_field)
        .unwrap_or(0);

    let move_focus = |current: usize, delta: isize| -> usize {
        let len = fields.len() as isize;
        let next = (current as isize + delta).rem_euclid(len);
        next as usize
    };

    match key.code {
        KeyCode::Esc => (),
        KeyCode::Tab | KeyCode::Down => {
            focus_index = move_focus(focus_index, 1);
            state.focused_field = fields[focus_index];
        }
        KeyCode::BackTab | KeyCode::Up => {
            focus_index = move_focus(focus_index, -1);
            state.focused_field = fields[focus_index];
        }
        KeyCode::Left if state.focused_field == CategoryInputField::Confirm => {
            state.focused_field = CategoryInputField::Cancel;
        }
        KeyCode::Right if state.focused_field == CategoryInputField::Cancel => {
            state.focused_field = CategoryInputField::Confirm;
        }
        KeyCode::Backspace => {
            if state.focused_field == CategoryInputField::Name {
                state.name_input.pop();
            }
        }
        KeyCode::Enter => {
            *follow_up = Some(match state.focused_field {
                CategoryInputField::Cancel => Message::DismissDialog,
                _ => Message::SubmitCategoryInput,
            });
        }
        KeyCode::Char(ch) => {
            if state.focused_field == CategoryInputField::Name {
                state.name_input.push(ch);
            }
        }
        _ => {}
    }
}

fn handle_delete_category_dialog_key(
    state: &mut DeleteCategoryDialogState,
    key: KeyEvent,
    follow_up: &mut Option<Message>,
) {
    match key.code {
        KeyCode::Esc => (),
        KeyCode::Left | KeyCode::Char('h') => {
            state.focused_field = match state.focused_field {
                DeleteCategoryField::Delete => DeleteCategoryField::Cancel,
                DeleteCategoryField::Cancel => DeleteCategoryField::Delete,
            };
        }
        KeyCode::Right | KeyCode::Char('l') | KeyCode::Tab => {
            state.focused_field = match state.focused_field {
                DeleteCategoryField::Delete => DeleteCategoryField::Cancel,
                DeleteCategoryField::Cancel => DeleteCategoryField::Delete,
            };
        }
        KeyCode::Enter => {
            *follow_up = Some(match state.focused_field {
                DeleteCategoryField::Delete => Message::ConfirmDeleteCategory,
                DeleteCategoryField::Cancel => Message::DismissDialog,
            });
        }
        _ => {}
    }
}

fn handle_delete_task_dialog_key(
    state: &mut DeleteTaskDialogState,
    key: KeyEvent,
    follow_up: &mut Option<Message>,
) {
    match key.code {
        KeyCode::Esc => (),
        KeyCode::Left | KeyCode::Char('h') => {
            state.focused_field = match state.focused_field {
                DeleteTaskField::KillTmux => DeleteTaskField::Cancel,
                DeleteTaskField::RemoveWorktree => DeleteTaskField::KillTmux,
                DeleteTaskField::DeleteBranch => DeleteTaskField::RemoveWorktree,
                DeleteTaskField::Delete => DeleteTaskField::DeleteBranch,
                DeleteTaskField::Cancel => DeleteTaskField::Delete,
            };
        }
        KeyCode::Right | KeyCode::Char('l') | KeyCode::Tab => {
            state.focused_field = match state.focused_field {
                DeleteTaskField::KillTmux => DeleteTaskField::RemoveWorktree,
                DeleteTaskField::RemoveWorktree => DeleteTaskField::DeleteBranch,
                DeleteTaskField::DeleteBranch => DeleteTaskField::Delete,
                DeleteTaskField::Delete => DeleteTaskField::Cancel,
                DeleteTaskField::Cancel => DeleteTaskField::KillTmux,
            };
        }
        KeyCode::Enter => {
            *follow_up = Some(match state.focused_field {
                DeleteTaskField::Delete => Message::ConfirmDeleteTask,
                DeleteTaskField::Cancel => Message::DismissDialog,
                _ => Message::DismissDialog,
            });
        }
        KeyCode::Char(' ') => {
            match state.focused_field {
                DeleteTaskField::KillTmux => state.kill_tmux = !state.kill_tmux,
                DeleteTaskField::RemoveWorktree => state.remove_worktree = !state.remove_worktree,
                DeleteTaskField::DeleteBranch => state.delete_branch = !state.delete_branch,
                _ => {}
            };
        }
        _ => {}
    }
}

fn handle_worktree_not_found_dialog_key(
    state: &mut WorktreeNotFoundDialogState,
    key: KeyEvent,
    follow_up: &mut Option<Message>,
) {
    match key.code {
        KeyCode::Esc => (),
        KeyCode::Left | KeyCode::Char('h') => {
            state.focused_field = match state.focused_field {
                WorktreeNotFoundField::Recreate => WorktreeNotFoundField::Cancel,
                WorktreeNotFoundField::MarkBroken => WorktreeNotFoundField::Recreate,
                WorktreeNotFoundField::Cancel => WorktreeNotFoundField::MarkBroken,
            };
        }
        KeyCode::Right | KeyCode::Char('l') | KeyCode::Tab => {
            state.focused_field = match state.focused_field {
                WorktreeNotFoundField::Recreate => WorktreeNotFoundField::MarkBroken,
                WorktreeNotFoundField::MarkBroken => WorktreeNotFoundField::Cancel,
                WorktreeNotFoundField::Cancel => WorktreeNotFoundField::Recreate,
            };
        }
        KeyCode::Enter => {
            *follow_up = Some(match state.focused_field {
                WorktreeNotFoundField::Recreate => Message::WorktreeNotFoundRecreate,
                WorktreeNotFoundField::MarkBroken => Message::WorktreeNotFoundMarkBroken,
                WorktreeNotFoundField::Cancel => Message::DismissDialog,
            });
        }
        _ => {}
    }
}

fn repo_default_base(repo: &Repo) -> String {
    use super::runtime::CreateTaskRuntime;
    repo.default_base
        .clone()
        .filter(|base| !base.trim().is_empty())
        .unwrap_or_else(|| {
            CreateTaskRuntime::git_detect_default_branch(
                &super::runtime::RealCreateTaskRuntime,
                std::path::Path::new(&repo.path),
            )
        })
}
