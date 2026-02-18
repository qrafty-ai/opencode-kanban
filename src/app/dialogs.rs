//! Dialog handling logic for key events and dialog operations

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::db::Database;
use crate::types::{Category, Repo};

use super::messages::Message;
use super::state::{
    ActiveDialog, ArchiveTaskDialogState, CategoryColorDialogState, CategoryColorField,
    CategoryInputDialogState, CategoryInputField, ConfirmCancelField, ConfirmQuitDialogState,
    DeleteCategoryDialogState, DeleteTaskDialogState, DeleteTaskField, NewProjectDialogState,
    NewProjectField, NewTaskDialogState, NewTaskField, RenameProjectDialogState,
    RenameProjectField, RenameRepoDialogState, RenameRepoField, RepoSuggestionItem,
    RepoSuggestionKind, WorktreeNotFoundDialogState, WorktreeNotFoundField,
};

/// Handle key events when a dialog is active
pub fn handle_dialog_key(
    dialog: &mut ActiveDialog,
    key: KeyEvent,
    db: &crate::db::Database,
    repos: &mut [Repo],
    _categories: &mut [Category],
    _focused_column: &mut usize,
) -> Result<Option<Message>> {
    let mut follow_up: Option<Message> = None;

    match dialog {
        ActiveDialog::NewTask(state) => {
            handle_new_task_dialog_key(state, key, repos, db, &mut follow_up);
        }
        ActiveDialog::NewProject(state) => {
            handle_new_project_dialog_key(state, key, &mut follow_up);
        }
        ActiveDialog::RenameProject(state) => {
            handle_rename_project_dialog_key(state, key, &mut follow_up);
        }
        ActiveDialog::DeleteProject(_) => match key.code {
            KeyCode::Esc => follow_up = Some(Message::DismissDialog),
            KeyCode::Enter => follow_up = Some(Message::ConfirmDeleteProject),
            KeyCode::Left | KeyCode::Right | KeyCode::Char('h') | KeyCode::Char('l') => {
                follow_up = Some(Message::DismissDialog);
            }
            _ => {}
        },
        ActiveDialog::RenameRepo(state) => {
            handle_rename_repo_dialog_key(state, key, &mut follow_up);
        }
        ActiveDialog::DeleteRepo(_) => match key.code {
            KeyCode::Esc => follow_up = Some(Message::DismissDialog),
            KeyCode::Enter => follow_up = Some(Message::ConfirmDeleteRepo),
            KeyCode::Left | KeyCode::Right | KeyCode::Char('h') | KeyCode::Char('l') => {
                follow_up = Some(Message::DismissDialog);
            }
            _ => {}
        },
        ActiveDialog::CategoryInput(state) => {
            handle_category_input_dialog_key(state, key, &mut follow_up);
        }
        ActiveDialog::CategoryColor(state) => {
            handle_category_color_dialog_key(state, key, &mut follow_up);
        }
        ActiveDialog::DeleteCategory(state) => {
            handle_delete_category_dialog_key(state, key, &mut follow_up);
        }
        ActiveDialog::DeleteTask(state) => {
            handle_delete_task_dialog_key(state, key, &mut follow_up);
        }
        ActiveDialog::ArchiveTask(state) => {
            handle_archive_task_dialog_key(state, key, &mut follow_up);
        }
        ActiveDialog::ConfirmQuit(state) => {
            handle_confirm_quit_dialog_key(state, key, &mut follow_up);
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
    db: &Database,
    follow_up: &mut Option<Message>,
) {
    if state.repo_picker.is_some() {
        handle_repo_picker_key(state, key, repos, db);
        return;
    }

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
        KeyCode::Esc => {
            *follow_up = Some(Message::DismissDialog);
        }
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
                state.repo_idx = state.repo_idx.saturating_sub(1);
                if let Some(repo) = repos.get(state.repo_idx) {
                    state.base_input = repo_default_base(repo);
                }
            }
        }
        KeyCode::Right if state.focused_field == NewTaskField::Repo => {
            if !repos.is_empty() {
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
            if state.focused_field == NewTaskField::Repo {
                open_repo_picker(state, repos, db);
            } else {
                *follow_up = Some(match state.focused_field {
                    NewTaskField::Cancel => Message::DismissDialog,
                    _ => Message::CreateTask,
                });
            }
        }
        KeyCode::Char(ch) => match state.focused_field {
            NewTaskField::Branch => state.branch_input.push(ch),
            NewTaskField::Base => state.base_input.push(ch),
            NewTaskField::Title => state.title_input.push(ch),
            _ => {}
        },
        _ => {}
    }
}

fn open_repo_picker(state: &mut NewTaskDialogState, repos: &[Repo], db: &Database) {
    let query = if !state.repo_input.trim().is_empty() {
        state.repo_input.clone()
    } else {
        repos
            .get(state.repo_idx)
            .map(|repo| repo.path.clone())
            .unwrap_or_default()
    };

    let mut picker = super::state::RepoPickerDialogState {
        query,
        selected_index: 0,
        suggestions: Vec::new(),
    };
    refresh_repo_picker_suggestions(&mut picker, repos, db);
    state.repo_picker = Some(picker);
}

fn handle_repo_picker_key(
    state: &mut NewTaskDialogState,
    key: KeyEvent,
    repos: &[Repo],
    db: &Database,
) {
    let mut apply: Option<RepoSuggestionItem> = None;
    let mut dismiss = false;

    {
        let Some(picker) = state.repo_picker.as_mut() else {
            return;
        };

        match key.code {
            KeyCode::Esc => dismiss = true,
            KeyCode::Enter => {
                apply = picker.suggestions.get(picker.selected_index).cloned();
                dismiss = true;
            }
            KeyCode::Tab => {
                if let Some(selected) = picker.suggestions.get(picker.selected_index) {
                    picker.query = selected.value.clone();
                    refresh_repo_picker_suggestions(picker, repos, db);
                }
            }
            KeyCode::Up => move_picker_selection(picker, -1),
            KeyCode::Down => move_picker_selection(picker, 1),
            KeyCode::Char('k') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                move_picker_selection(picker, -1)
            }
            KeyCode::Char('j') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                move_picker_selection(picker, 1)
            }
            KeyCode::Backspace => {
                picker.query.pop();
                refresh_repo_picker_suggestions(picker, repos, db);
            }
            KeyCode::Char(ch)
                if !key.modifiers.contains(KeyModifiers::CONTROL)
                    && !key.modifiers.contains(KeyModifiers::ALT) =>
            {
                picker.query.push(ch);
                refresh_repo_picker_suggestions(picker, repos, db);
            }
            _ => {}
        }
    }

    if let Some(suggestion) = apply {
        apply_repo_suggestion(state, repos, &suggestion);
    }
    if dismiss {
        state.repo_picker = None;
    }
}

fn refresh_repo_picker_suggestions(
    picker: &mut super::state::RepoPickerDialogState,
    repos: &[Repo],
    db: &Database,
) {
    let previous_value = picker
        .suggestions
        .get(picker.selected_index)
        .map(|candidate| candidate.value.clone());
    picker.suggestions = build_repo_suggestions(picker.query.trim(), repos, db);
    if picker.suggestions.is_empty() {
        picker.selected_index = 0;
        return;
    }

    picker.selected_index = previous_value
        .and_then(|value| {
            picker
                .suggestions
                .iter()
                .position(|candidate| candidate.value == value)
        })
        .unwrap_or(0)
        .min(picker.suggestions.len() - 1);
}

fn move_picker_selection(picker: &mut super::state::RepoPickerDialogState, delta: isize) {
    if picker.suggestions.is_empty() {
        picker.selected_index = 0;
        return;
    }

    let len = picker.suggestions.len() as isize;
    let next = (picker.selected_index as isize + delta).rem_euclid(len);
    picker.selected_index = next as usize;
}

fn apply_repo_suggestion(
    state: &mut NewTaskDialogState,
    repos: &[Repo],
    suggestion: &RepoSuggestionItem,
) {
    state.repo_input = suggestion.value.clone();

    let repo_idx_from_suggestion = match suggestion.kind {
        RepoSuggestionKind::KnownRepo { repo_idx } => Some(repo_idx),
        RepoSuggestionKind::FolderPath => find_repo_idx_by_path_value(repos, &suggestion.value),
    };

    if let Some(repo_idx) = repo_idx_from_suggestion {
        state.repo_idx = repo_idx;
        if let Some(repo) = repos.get(repo_idx) {
            state.base_input = repo_default_base(repo);
        }
    }
}

fn find_repo_idx_by_path_value(repos: &[Repo], value: &str) -> Option<usize> {
    let normalized_value = normalize_path_value(value);
    repos
        .iter()
        .position(|repo| normalize_path_value(&repo.path) == normalized_value)
}

fn normalize_path_value(value: &str) -> String {
    value
        .trim_end_matches(std::path::MAIN_SEPARATOR)
        .to_string()
}

fn build_repo_suggestions(query: &str, repos: &[Repo], db: &Database) -> Vec<RepoSuggestionItem> {
    let mut suggestions = Vec::new();
    let mut seen_values = HashSet::new();
    let normalized_query = query.trim();

    for folder_path in folder_suggestion_paths(normalized_query) {
        let key = normalize_path_value(&folder_path).to_ascii_lowercase();
        if seen_values.insert(key) {
            suggestions.push(RepoSuggestionItem {
                label: folder_label(&folder_path),
                value: folder_path,
                kind: RepoSuggestionKind::FolderPath,
            });
        }
    }

    let usage = super::repo_selection_usage_map(db);
    for repo_idx in super::rank_repos_for_query(normalized_query, repos, &usage) {
        if let Some(repo) = repos.get(repo_idx)
            && seen_values.insert(normalize_path_value(&repo.path).to_ascii_lowercase())
        {
            suggestions.push(RepoSuggestionItem {
                label: repo.name.clone(),
                value: repo.path.clone(),
                kind: RepoSuggestionKind::KnownRepo { repo_idx },
            });
        }
    }

    suggestions
}

fn folder_label(path: &str) -> String {
    let path = Path::new(path);
    path.file_name()
        .and_then(|name| name.to_str())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| path.display().to_string())
}

fn folder_suggestion_paths(query: &str) -> Vec<String> {
    let expanded = expand_home_prefix(query);
    if expanded.is_empty() {
        return Vec::new();
    }

    let path = PathBuf::from(&expanded);
    if path.is_dir() {
        if expanded.ends_with(std::path::MAIN_SEPARATOR) {
            return list_directory_suggestions(&path, None);
        }

        return parent_and_matching_directory_suggestions(&path);
    }

    let Some(parent) = path.parent() else {
        return Vec::new();
    };
    let prefix = path
        .file_name()
        .and_then(|segment| segment.to_str())
        .unwrap_or_default();
    list_directory_suggestions(parent, Some(prefix))
}

fn list_directory_suggestions(parent: &Path, prefix: Option<&str>) -> Vec<String> {
    let mut out: Vec<String> = fs::read_dir(parent)
        .ok()
        .into_iter()
        .flat_map(|entries| entries.filter_map(Result::ok))
        .filter_map(|entry| {
            let path = entry.path();
            if !path.is_dir() {
                return None;
            }

            if let Some(prefix) = prefix {
                let file_name = path.file_name()?.to_str()?;
                if !file_name.starts_with(prefix) {
                    return None;
                }
            }

            Some(path)
        })
        .map(|path| format!("{}{sep}", path.display(), sep = std::path::MAIN_SEPARATOR))
        .collect();

    out.sort();
    out
}

fn parent_and_matching_directory_suggestions(path: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();

    let mut push_unique = |value: String| {
        let key = value.to_ascii_lowercase();
        if seen.insert(key) {
            out.push(value);
        }
    };

    push_unique(format!(
        "{}{sep}",
        path.display(),
        sep = std::path::MAIN_SEPARATOR
    ));

    if let Some(parent) = path.parent() {
        push_unique(format!(
            "{}{sep}",
            parent.display(),
            sep = std::path::MAIN_SEPARATOR
        ));

        let prefix = path
            .file_name()
            .and_then(|segment| segment.to_str())
            .unwrap_or_default();
        let exact_name = prefix.to_ascii_lowercase();
        let mut candidates = list_directory_suggestions(parent, Some(prefix));
        candidates.sort_by(|left, right| {
            let left_name = Path::new(left)
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default()
                .to_ascii_lowercase();
            let right_name = Path::new(right)
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default()
                .to_ascii_lowercase();

            let left_exact = left_name == exact_name;
            let right_exact = right_name == exact_name;
            right_exact
                .cmp(&left_exact)
                .then_with(|| left_name.len().cmp(&right_name.len()))
                .then_with(|| left_name.cmp(&right_name))
        });

        for candidate in candidates {
            push_unique(candidate);
        }
    }

    out
}

fn expand_home_prefix(value: &str) -> String {
    if let Some(stripped) = value.strip_prefix('~') {
        if stripped.is_empty() {
            if let Some(home) = dirs::home_dir() {
                return home.display().to_string();
            }
            return value.to_string();
        }

        if let Some(remainder) = stripped.strip_prefix(std::path::MAIN_SEPARATOR)
            && let Some(home) = dirs::home_dir()
        {
            return home.join(remainder).display().to_string();
        }
    }

    value.to_string()
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
        KeyCode::Esc => {
            *follow_up = Some(Message::DismissDialog);
        }
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
        KeyCode::Esc => {
            *follow_up = Some(Message::DismissDialog);
        }
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
    handle_confirm_cancel_dialog_key(
        &mut state.focused_field,
        key,
        Message::ConfirmDeleteCategory,
        Message::DismissDialog,
        follow_up,
    );
}

fn handle_category_color_dialog_key(
    state: &mut CategoryColorDialogState,
    key: KeyEvent,
    follow_up: &mut Option<Message>,
) {
    let focus_next = |focused_field: CategoryColorField| match focused_field {
        CategoryColorField::Palette => CategoryColorField::Confirm,
        CategoryColorField::Confirm => CategoryColorField::Cancel,
        CategoryColorField::Cancel => CategoryColorField::Palette,
    };
    let focus_prev = |focused_field: CategoryColorField| match focused_field {
        CategoryColorField::Palette => CategoryColorField::Cancel,
        CategoryColorField::Confirm => CategoryColorField::Palette,
        CategoryColorField::Cancel => CategoryColorField::Confirm,
    };

    match key.code {
        KeyCode::Esc => {
            *follow_up = Some(Message::DismissDialog);
        }
        KeyCode::Tab => {
            state.focused_field = focus_next(state.focused_field);
        }
        KeyCode::BackTab => {
            state.focused_field = focus_prev(state.focused_field);
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if matches!(state.focused_field, CategoryColorField::Palette) {
                state.selected_index = state
                    .selected_index
                    .saturating_add(1)
                    .min(super::state::CATEGORY_COLOR_PALETTE.len().saturating_sub(1));
            } else {
                state.focused_field = focus_next(state.focused_field);
            }
        }
        KeyCode::Up | KeyCode::Char('k') => {
            if matches!(state.focused_field, CategoryColorField::Palette) {
                state.selected_index = state.selected_index.saturating_sub(1);
            } else {
                state.focused_field = focus_prev(state.focused_field);
            }
        }
        KeyCode::Left | KeyCode::Char('h') => match state.focused_field {
            CategoryColorField::Palette => {
                state.selected_index = state.selected_index.saturating_sub(1);
            }
            CategoryColorField::Confirm => {
                state.focused_field = CategoryColorField::Cancel;
            }
            CategoryColorField::Cancel => {
                state.focused_field = CategoryColorField::Confirm;
            }
        },
        KeyCode::Right | KeyCode::Char('l') => match state.focused_field {
            CategoryColorField::Palette => {
                state.selected_index = state
                    .selected_index
                    .saturating_add(1)
                    .min(super::state::CATEGORY_COLOR_PALETTE.len().saturating_sub(1));
            }
            CategoryColorField::Confirm => {
                state.focused_field = CategoryColorField::Cancel;
            }
            CategoryColorField::Cancel => {
                state.focused_field = CategoryColorField::Confirm;
            }
        },
        KeyCode::Enter => {
            *follow_up = Some(match state.focused_field {
                CategoryColorField::Cancel => Message::DismissDialog,
                _ => Message::ConfirmCategoryColor,
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
        KeyCode::Esc => {
            *follow_up = Some(Message::DismissDialog);
        }
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

fn handle_archive_task_dialog_key(
    state: &mut ArchiveTaskDialogState,
    key: KeyEvent,
    follow_up: &mut Option<Message>,
) {
    handle_confirm_cancel_dialog_key(
        &mut state.focused_field,
        key,
        Message::ConfirmArchiveTask,
        Message::DismissDialog,
        follow_up,
    );
}

fn handle_confirm_quit_dialog_key(
    state: &mut ConfirmQuitDialogState,
    key: KeyEvent,
    follow_up: &mut Option<Message>,
) {
    handle_confirm_cancel_dialog_key(
        &mut state.focused_field,
        key,
        Message::ConfirmQuit,
        Message::CancelQuit,
        follow_up,
    );
}

fn toggle_confirm_cancel_field(field: &mut ConfirmCancelField) {
    *field = match *field {
        ConfirmCancelField::Confirm => ConfirmCancelField::Cancel,
        ConfirmCancelField::Cancel => ConfirmCancelField::Confirm,
    };
}

fn handle_confirm_cancel_dialog_key(
    focused_field: &mut ConfirmCancelField,
    key: KeyEvent,
    confirm_message: Message,
    cancel_message: Message,
    follow_up: &mut Option<Message>,
) {
    match key.code {
        KeyCode::Esc => {
            *follow_up = Some(cancel_message);
        }
        KeyCode::Left
        | KeyCode::Char('h')
        | KeyCode::Up
        | KeyCode::Char('k')
        | KeyCode::Right
        | KeyCode::Char('l')
        | KeyCode::Down
        | KeyCode::Char('j')
        | KeyCode::Tab
        | KeyCode::BackTab => {
            toggle_confirm_cancel_field(focused_field);
        }
        KeyCode::Enter => {
            *follow_up = Some(match focused_field {
                ConfirmCancelField::Confirm => confirm_message,
                ConfirmCancelField::Cancel => cancel_message,
            });
        }
        _ => {}
    }
}

fn handle_rename_project_dialog_key(
    state: &mut RenameProjectDialogState,
    key: KeyEvent,
    follow_up: &mut Option<Message>,
) {
    let fields = [
        RenameProjectField::Name,
        RenameProjectField::Confirm,
        RenameProjectField::Cancel,
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
        KeyCode::Esc => {
            *follow_up = Some(Message::DismissDialog);
        }
        KeyCode::Tab | KeyCode::Down => {
            focus_index = move_focus(focus_index, 1);
            state.focused_field = fields[focus_index];
        }
        KeyCode::BackTab | KeyCode::Up => {
            focus_index = move_focus(focus_index, -1);
            state.focused_field = fields[focus_index];
        }
        KeyCode::Left if state.focused_field == RenameProjectField::Confirm => {
            state.focused_field = RenameProjectField::Cancel;
        }
        KeyCode::Right if state.focused_field == RenameProjectField::Cancel => {
            state.focused_field = RenameProjectField::Confirm;
        }
        KeyCode::Backspace => {
            if state.focused_field == RenameProjectField::Name {
                state.name_input.pop();
            }
        }
        KeyCode::Enter => {
            *follow_up = Some(match state.focused_field {
                RenameProjectField::Cancel => Message::DismissDialog,
                _ => Message::ConfirmRenameProject,
            });
        }
        KeyCode::Char(ch) => {
            if state.focused_field == RenameProjectField::Name {
                state.name_input.push(ch);
            }
        }
        _ => {}
    }
}

fn handle_rename_repo_dialog_key(
    state: &mut RenameRepoDialogState,
    key: KeyEvent,
    follow_up: &mut Option<Message>,
) {
    let fields = [
        RenameRepoField::Name,
        RenameRepoField::Confirm,
        RenameRepoField::Cancel,
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
        KeyCode::Esc => {
            *follow_up = Some(Message::DismissDialog);
        }
        KeyCode::Tab | KeyCode::Down => {
            focus_index = move_focus(focus_index, 1);
            state.focused_field = fields[focus_index];
        }
        KeyCode::BackTab | KeyCode::Up => {
            focus_index = move_focus(focus_index, -1);
            state.focused_field = fields[focus_index];
        }
        KeyCode::Left if state.focused_field == RenameRepoField::Confirm => {
            state.focused_field = RenameRepoField::Cancel;
        }
        KeyCode::Right if state.focused_field == RenameRepoField::Cancel => {
            state.focused_field = RenameRepoField::Confirm;
        }
        KeyCode::Backspace => {
            if state.focused_field == RenameRepoField::Name {
                state.name_input.pop();
            }
        }
        KeyCode::Enter => {
            *follow_up = Some(match state.focused_field {
                RenameRepoField::Cancel => Message::DismissDialog,
                _ => Message::ConfirmRenameRepo,
            });
        }
        KeyCode::Char(ch) => {
            if state.focused_field == RenameRepoField::Name {
                state.name_input.push(ch);
            }
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

#[cfg(test)]
mod tests {
    use super::*;

    use crossterm::event::KeyModifiers;
    use tempfile::TempDir;
    use uuid::Uuid;

    fn key_tab() -> KeyEvent {
        KeyEvent::new(KeyCode::Tab, KeyModifiers::empty())
    }

    fn key_down() -> KeyEvent {
        KeyEvent::new(KeyCode::Down, KeyModifiers::empty())
    }

    fn key_enter() -> KeyEvent {
        KeyEvent::new(KeyCode::Enter, KeyModifiers::empty())
    }

    fn test_repo(id: Uuid, name: &str, default_base: &str) -> Repo {
        Repo {
            id,
            path: format!("/tmp/{name}"),
            name: name.to_string(),
            default_base: Some(default_base.to_string()),
            remote_url: None,
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
        }
    }

    fn repo_focused_state() -> NewTaskDialogState {
        NewTaskDialogState {
            repo_idx: 0,
            repo_input: String::new(),
            repo_picker: None,
            branch_input: String::new(),
            base_input: "main".to_string(),
            title_input: String::new(),
            ensure_base_up_to_date: true,
            loading_message: None,
            focused_field: NewTaskField::Repo,
        }
    }

    #[test]
    fn enter_on_repo_opens_picker_overlay() -> Result<()> {
        let db = Database::open(":memory:")?;
        let mut repos = vec![
            test_repo(Uuid::new_v4(), "frontend-app", "main"),
            test_repo(Uuid::new_v4(), "backend-api", "develop"),
        ];
        let mut state = repo_focused_state();
        let mut follow_up = None;

        handle_new_task_dialog_key(
            &mut state,
            key_enter(),
            repos.as_mut_slice(),
            &db,
            &mut follow_up,
        );

        assert!(state.repo_picker.is_some());
        assert_eq!(state.focused_field, NewTaskField::Repo);
        assert!(follow_up.is_none());
        Ok(())
    }

    #[test]
    fn picker_enter_selects_highlighted_repo() -> Result<()> {
        let db = Database::open(":memory:")?;
        let first = test_repo(Uuid::new_v4(), "frontend-app", "main");
        let second = test_repo(Uuid::new_v4(), "backend-api", "release");
        let mut repos = vec![first, second.clone()];

        let mut state = repo_focused_state();
        state.repo_input = "backend".to_string();
        let mut follow_up = None;

        handle_new_task_dialog_key(
            &mut state,
            key_enter(),
            repos.as_mut_slice(),
            &db,
            &mut follow_up,
        );
        handle_new_task_dialog_key(
            &mut state,
            key_enter(),
            repos.as_mut_slice(),
            &db,
            &mut follow_up,
        );

        assert_eq!(state.repo_idx, 1);
        assert_eq!(state.base_input, "release");
        assert_eq!(state.repo_input, second.path);
        assert!(state.repo_picker.is_none());
        Ok(())
    }

    #[test]
    fn picker_down_moves_selection_and_enter_applies() -> Result<()> {
        let db = Database::open(":memory:")?;
        let first = test_repo(Uuid::new_v4(), "api-admin", "main");
        let second = test_repo(Uuid::new_v4(), "api-gateway", "develop");
        let mut repos = vec![first, second.clone()];

        let mut state = repo_focused_state();
        state.repo_input = "api".to_string();

        let mut follow_up = None;
        handle_new_task_dialog_key(
            &mut state,
            key_enter(),
            repos.as_mut_slice(),
            &db,
            &mut follow_up,
        );
        handle_new_task_dialog_key(
            &mut state,
            key_down(),
            repos.as_mut_slice(),
            &db,
            &mut follow_up,
        );
        handle_new_task_dialog_key(
            &mut state,
            key_enter(),
            repos.as_mut_slice(),
            &db,
            &mut follow_up,
        );

        assert_eq!(state.repo_idx, 1);
        assert_eq!(state.base_input, "develop");
        assert_eq!(state.repo_input, second.path);
        assert!(follow_up.is_none());
        Ok(())
    }

    #[test]
    fn picker_tab_completes_to_selected_suggestion_value() -> Result<()> {
        let db = Database::open(":memory:")?;
        let first = test_repo(Uuid::new_v4(), "frontend-app", "main");
        let second = test_repo(Uuid::new_v4(), "backend-api", "develop");
        let mut repos = vec![first, second.clone()];

        let mut state = repo_focused_state();
        state.repo_input = "back".to_string();
        let mut follow_up = None;

        handle_new_task_dialog_key(
            &mut state,
            key_enter(),
            repos.as_mut_slice(),
            &db,
            &mut follow_up,
        );
        handle_new_task_dialog_key(
            &mut state,
            key_tab(),
            repos.as_mut_slice(),
            &db,
            &mut follow_up,
        );

        let picker = state
            .repo_picker
            .as_ref()
            .expect("picker should remain open");
        assert_eq!(picker.query, second.path);
        Ok(())
    }

    #[test]
    fn folder_suggestion_paths_completes_matching_directory_prefix() -> Result<()> {
        let temp = TempDir::new()?;
        let target = temp.path().join("codes");
        std::fs::create_dir_all(&target)?;

        let query = format!("{}/co", temp.path().display());
        let suggestions = folder_suggestion_paths(&query);
        let expected = format!("{}{sep}", target.display(), sep = std::path::MAIN_SEPARATOR);
        assert!(suggestions.contains(&expected));
        Ok(())
    }

    #[test]
    fn folder_suggestion_paths_lists_all_directories_under_path() -> Result<()> {
        let temp = TempDir::new()?;
        let root = temp.path().join("workspace");
        let alpha = root.join("alpha");
        let beta = root.join("beta");
        let file = root.join("README.md");
        std::fs::create_dir_all(&alpha)?;
        std::fs::create_dir_all(&beta)?;
        std::fs::write(&file, "not-a-folder")?;

        let query = format!("{}{sep}", root.display(), sep = std::path::MAIN_SEPARATOR);
        let suggestions = folder_suggestion_paths(&query);
        let alpha_entry = format!("{}{sep}", alpha.display(), sep = std::path::MAIN_SEPARATOR);
        let beta_entry = format!("{}{sep}", beta.display(), sep = std::path::MAIN_SEPARATOR);

        assert_eq!(suggestions.len(), 2);
        assert!(suggestions.contains(&alpha_entry));
        assert!(suggestions.contains(&beta_entry));
        Ok(())
    }

    #[test]
    fn folder_suggestion_paths_existing_dir_without_slash_includes_parent_and_self() -> Result<()> {
        let temp = TempDir::new()?;
        let root = temp.path().join("workspace");
        let tea = root.join("tea");
        let tea_worktrees = root.join("tea-worktrees");
        let tea_new = root.join("tea_new_description");
        let cache = tea.join(".cache");
        std::fs::create_dir_all(&tea_worktrees)?;
        std::fs::create_dir_all(&tea_new)?;
        std::fs::create_dir_all(&cache)?;

        let query = tea.display().to_string();
        let suggestions = folder_suggestion_paths(&query);

        let parent_entry = format!("{}{sep}", root.display(), sep = std::path::MAIN_SEPARATOR);
        let tea_entry = format!("{}{sep}", tea.display(), sep = std::path::MAIN_SEPARATOR);
        let cache_entry = format!("{}{sep}", cache.display(), sep = std::path::MAIN_SEPARATOR);

        assert_eq!(suggestions.first(), Some(&tea_entry));
        assert!(suggestions.contains(&parent_entry));
        assert!(suggestions.contains(&tea_entry));
        assert!(!suggestions.contains(&cache_entry));
        Ok(())
    }
}
