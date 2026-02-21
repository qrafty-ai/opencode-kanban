pub mod actions;
mod core;
pub mod dialogs;
mod input;
pub mod interaction;
mod log;
pub mod messages;
mod navigation;
pub mod polling;
pub mod runtime;
mod side_panel;
pub mod state;
mod update;
pub mod workflows;

pub(crate) use self::core::load_project_detail;
pub use self::core::{App, ProjectDetailCache, SubagentTodoSummary};

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use crossterm::event::MouseEvent;
use tokio::task::JoinHandle;
use tracing::warn;
use tuirealm::ratatui::layout::Rect;
use tuirealm::ratatui::widgets::{ListState, ScrollbarState};
use uuid::Uuid;

use self::interaction::{InteractionKind, InteractionMap};
use self::log::{log_kind_label, log_role_label, log_time_label};
pub use self::messages::Message;
pub use self::state::{
    ActiveDialog, ArchiveTaskDialogState, CATEGORY_COLOR_PALETTE, CategoryColorDialogState,
    CategoryColorField, CategoryInputDialogState, CategoryInputField, CategoryInputMode,
    ConfirmCancelField, ConfirmQuitDialogState, ContextMenuItem, ContextMenuState,
    DeleteCategoryDialogState, DeleteProjectDialogState, DeleteRepoDialogState,
    DeleteTaskDialogState, DeleteTaskField, DetailFocus, ErrorDialogState, MoveTaskDialogState,
    NewProjectDialogState, NewProjectField, NewTaskDialogState, NewTaskField,
    RenameProjectDialogState, RenameProjectField, RenameRepoDialogState, RenameRepoField,
    RepoPickerDialogState, RepoPickerTarget, RepoSuggestionItem, RepoSuggestionKind,
    RepoUnavailableDialogState, SettingsSection, SettingsViewState, TodoVisualizationMode, View,
    ViewMode, WorktreeNotFoundDialogState, WorktreeNotFoundField, category_color_label,
    normalize_category_color_key,
};

use crate::command_palette::{CommandPaletteState, all_commands};
use crate::db::Database;
use crate::git::{GitChangeSummary, git_delete_branch, git_remove_worktree};
use crate::keybindings::Keybindings;
use crate::opencode::{OpenCodeServerManager, Status, ensure_server_ready};
use crate::projects::{self, ProjectInfo};
use crate::theme::{Theme, ThemePreset};
use crate::tmux::tmux_kill_session;
use crate::types::{Category, Repo, SessionMessageItem, SessionTodoItem, Task};

use self::runtime::{RealCreateTaskRuntime, RealRecoveryRuntime, RecoveryRuntime};
use self::state::AttachTaskResult;
use self::workflows::{
    attach_task_with_runtime, create_task_error_dialog_state, create_task_pipeline_with_runtime,
    rank_repos_for_query, reconcile_startup_tasks, repo_selection_usage_map,
};

const GG_SEQUENCE_TIMEOUT: Duration = Duration::from_millis(500);

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum SidePanelRow {
    CategoryHeader {
        column_index: usize,
        category_id: Uuid,
        category_name: String,
        category_color: Option<String>,
        total_tasks: usize,
        visible_tasks: usize,
        collapsed: bool,
    },
    Task {
        column_index: usize,
        index_in_column: usize,
        category_id: Uuid,
        task: Box<Task>,
    },
}

impl App {
    fn move_category_left(&mut self) -> Result<()> {
        if self.categories.len() < 2 || self.focused_column == 0 {
            return Ok(());
        }

        let current_index = self.focused_column.min(self.categories.len() - 1);
        if current_index == 0 {
            return Ok(());
        }

        let current = self.categories[current_index].clone();
        let left = self.categories[current_index - 1].clone();

        self.db
            .update_category_position(current.id, left.position)?;
        self.db
            .update_category_position(left.id, current.position)?;

        self.refresh_data()?;
        if let Some(index) = self
            .categories
            .iter()
            .position(|category| category.id == current.id)
        {
            self.focused_column = index;
            self.selected_task_per_column.entry(index).or_insert(0);
        }

        Ok(())
    }

    fn move_category_right(&mut self) -> Result<()> {
        if self.categories.len() < 2 {
            return Ok(());
        }

        let current_index = self.focused_column.min(self.categories.len() - 1);
        if current_index + 1 >= self.categories.len() {
            return Ok(());
        }

        let current = self.categories[current_index].clone();
        let right = self.categories[current_index + 1].clone();

        self.db
            .update_category_position(current.id, right.position)?;
        self.db
            .update_category_position(right.id, current.position)?;

        self.refresh_data()?;
        if let Some(index) = self
            .categories
            .iter()
            .position(|category| category.id == current.id)
        {
            self.focused_column = index;
            self.selected_task_per_column.entry(index).or_insert(0);
        }

        Ok(())
    }

    fn move_task_left(&mut self) -> Result<()> {
        if self.focused_column == 0 {
            return Ok(());
        }
        let Some(task) = self.selected_task() else {
            return Ok(());
        };
        let target_column = self.focused_column - 1;
        let target_category = &self.categories[target_column];
        self.db
            .update_task_category(task.id, target_category.id, 0)?;
        self.focused_column = target_column;
        self.selected_task_per_column.insert(target_column, 0);
        self.refresh_data()
    }

    fn move_task_right(&mut self) -> Result<()> {
        if self.focused_column >= self.categories.len() - 1 {
            return Ok(());
        }
        let Some(task) = self.selected_task() else {
            return Ok(());
        };
        let target_column = self.focused_column + 1;
        let target_category = &self.categories[target_column];
        self.db
            .update_task_category(task.id, target_category.id, 0)?;
        self.focused_column = target_column;
        self.selected_task_per_column.insert(target_column, 0);
        self.refresh_data()
    }

    fn move_task_up(&mut self) -> Result<()> {
        let column_index = self.focused_column;
        let Some(category) = self.categories.get(column_index) else {
            return Ok(());
        };
        let mut tasks: Vec<_> = self
            .tasks
            .iter()
            .filter(|t| t.category_id == category.id)
            .cloned()
            .collect();
        tasks.sort_by_key(|t| t.position);
        if tasks.len() < 2 {
            return Ok(());
        }
        let selected = self
            .selected_task_per_column
            .get(&column_index)
            .copied()
            .unwrap_or(0)
            .min(tasks.len() - 1);
        if selected == 0 {
            return Ok(());
        }
        tasks.swap(selected - 1, selected);
        for (idx, task) in tasks.iter().enumerate() {
            self.db.update_task_position(task.id, idx as i64)?;
        }
        self.selected_task_per_column
            .insert(column_index, selected - 1);
        self.refresh_data()
    }

    fn move_task_down(&mut self) -> Result<()> {
        let column_index = self.focused_column;
        let Some(category) = self.categories.get(column_index) else {
            return Ok(());
        };
        let mut tasks: Vec<_> = self
            .tasks
            .iter()
            .filter(|t| t.category_id == category.id)
            .cloned()
            .collect();
        tasks.sort_by_key(|t| t.position);
        if tasks.len() < 2 {
            return Ok(());
        }
        let selected = self
            .selected_task_per_column
            .get(&column_index)
            .copied()
            .unwrap_or(0)
            .min(tasks.len() - 1);
        if selected + 1 >= tasks.len() {
            return Ok(());
        }
        tasks.swap(selected, selected + 1);
        for (idx, task) in tasks.iter().enumerate() {
            self.db.update_task_position(task.id, idx as i64)?;
        }
        self.selected_task_per_column
            .insert(column_index, selected + 1);
        self.refresh_data()
    }

    fn open_delete_category_dialog(&mut self) -> Result<()> {
        let Some(category) = self.categories.get(self.focused_column) else {
            return Ok(());
        };

        let task_count = self.tasks_in_column(self.focused_column);
        self.active_dialog = ActiveDialog::DeleteCategory(DeleteCategoryDialogState {
            category_id: category.id,
            category_name: category.name.clone(),
            task_count,
            focused_field: ConfirmCancelField::Cancel,
        });
        Ok(())
    }

    fn open_category_color_dialog(&mut self) {
        let Some(category) = self.categories.get(self.focused_column) else {
            return;
        };

        self.active_dialog = ActiveDialog::CategoryColor(CategoryColorDialogState {
            category_id: category.id,
            category_name: category.name.clone(),
            selected_index: palette_index_for(category.color.as_deref()),
            focused_field: CategoryColorField::Palette,
        });
    }

    fn confirm_category_color(&mut self) -> Result<()> {
        let ActiveDialog::CategoryColor(state) = self.active_dialog.clone() else {
            return Ok(());
        };

        let selected = CATEGORY_COLOR_PALETTE
            .get(state.selected_index)
            .copied()
            .unwrap_or(None)
            .map(str::to_string);
        self.db
            .update_category_color(state.category_id, selected)
            .context("failed to update category color")?;
        self.active_dialog = ActiveDialog::None;
        self.refresh_data()?;
        Ok(())
    }

    fn open_delete_task_dialog(&mut self) -> Result<()> {
        let task = if self.current_view == View::Archive {
            self.selected_archived_task()
        } else {
            self.selected_task()
        };
        let Some(task) = task else {
            return Ok(());
        };

        self.active_dialog = ActiveDialog::DeleteTask(DeleteTaskDialogState {
            task_id: task.id,
            task_title: task.title.clone(),
            task_branch: task.branch.clone(),
            kill_tmux: true,
            remove_worktree: false,
            delete_branch: false,
            confirm_destructive: false,
            focused_field: DeleteTaskField::Cancel,
        });
        Ok(())
    }

    fn open_archive_task_dialog(&mut self) -> Result<()> {
        if self.current_view != View::Board {
            return Ok(());
        }

        let Some(task) = self.selected_task() else {
            return Ok(());
        };

        self.active_dialog = ActiveDialog::ArchiveTask(ArchiveTaskDialogState {
            task_id: task.id,
            task_title: task.title,
            focused_field: ConfirmCancelField::Cancel,
        });
        Ok(())
    }

    fn confirm_category_input(&mut self) -> Result<()> {
        let ActiveDialog::CategoryInput(state) = self.active_dialog.clone() else {
            return Ok(());
        };

        let name = state.name_input.trim();
        if name.is_empty() {
            self.active_dialog = ActiveDialog::Error(ErrorDialogState {
                title: "Invalid category".to_string(),
                detail: "Category name cannot be empty.".to_string(),
            });
            return Ok(());
        }

        match state.mode {
            CategoryInputMode::Add => {
                let next_position = self
                    .categories
                    .iter()
                    .map(|category| category.position)
                    .max()
                    .unwrap_or(-1)
                    + 1;
                let created = self.db.add_category(name, next_position, None)?;
                self.active_dialog = ActiveDialog::None;
                self.refresh_data()?;
                if let Some(index) = self.categories.iter().position(|c| c.id == created.id) {
                    self.focused_column = index;
                    self.selected_task_per_column.entry(index).or_insert(0);
                }
            }
            CategoryInputMode::Rename => {
                let Some(category_id) = state.category_id else {
                    return Ok(());
                };
                self.db.rename_category(category_id, name)?;
                self.active_dialog = ActiveDialog::None;
                self.refresh_data()?;
            }
        }

        Ok(())
    }

    fn confirm_delete_category(&mut self) -> Result<()> {
        let ActiveDialog::DeleteCategory(state) = self.active_dialog.clone() else {
            return Ok(());
        };

        if state.task_count > 0 {
            self.active_dialog = ActiveDialog::Error(ErrorDialogState {
                title: "Category not empty".to_string(),
                detail: format!(
                    "Cannot delete '{}' because it still contains {} task(s).",
                    state.category_name, state.task_count
                ),
            });
            return Ok(());
        }

        self.db.delete_category(state.category_id)?;
        self.active_dialog = ActiveDialog::None;
        self.refresh_data()?;
        Ok(())
    }

    fn confirm_delete_task(&mut self) -> Result<()> {
        let ActiveDialog::DeleteTask(mut state) = self.active_dialog.clone() else {
            return Ok(());
        };

        let destructive_cleanup_requested = state.remove_worktree || state.delete_branch;
        if destructive_cleanup_requested && !state.confirm_destructive {
            state.confirm_destructive = true;
            state.focused_field = DeleteTaskField::Delete;
            self.active_dialog = ActiveDialog::DeleteTask(state);
            return Ok(());
        }

        let task = self
            .tasks
            .iter()
            .find(|task| task.id == state.task_id)
            .cloned()
            .or_else(|| self.db.get_task(state.task_id).ok());
        let Some(task) = task else {
            self.active_dialog = ActiveDialog::None;
            return Ok(());
        };

        let repo = self.repo_for_task(&task);

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
            && let Some(r) = repo
            && !task.branch.is_empty()
        {
            let _ = git_delete_branch(Path::new(&r.path), &task.branch);
        }

        self.db.delete_task(state.task_id)?;
        self.active_dialog = ActiveDialog::None;
        self.refresh_data()?;
        if self.current_view == View::Archive {
            self.archived_tasks = self.db.list_archived_tasks()?;
            self.archive_selected_index = self
                .archive_selected_index
                .min(self.archived_tasks.len().saturating_sub(1));
        }
        Ok(())
    }

    fn confirm_archive_task(&mut self) -> Result<()> {
        let ActiveDialog::ArchiveTask(state) = self.active_dialog.clone() else {
            return Ok(());
        };

        self.db.archive_task(state.task_id)?;
        self.active_dialog = ActiveDialog::None;
        self.refresh_data()?;
        Ok(())
    }

    fn unarchive_selected_task(&mut self) -> Result<()> {
        if self.current_view != View::Archive {
            return Ok(());
        }

        let Some(task) = self.selected_archived_task() else {
            return Ok(());
        };

        self.db.unarchive_task(task.id)?;
        self.archived_tasks = self.db.list_archived_tasks()?;
        self.archive_selected_index = self
            .archive_selected_index
            .min(self.archived_tasks.len().saturating_sub(1));
        self.refresh_data()?;
        Ok(())
    }

    fn reconcile_startup_with_runtime(&mut self, runtime: &impl RecoveryRuntime) -> Result<()> {
        reconcile_startup_tasks(&self.db, &self.tasks, &self.repos, runtime)
    }

    fn attach_selected_task(&mut self) -> Result<()> {
        if self.current_view == View::Archive {
            return Ok(());
        }

        let Some(task) = self.selected_task() else {
            return Ok(());
        };
        let Some(repo) = self.repo_for_task(&task) else {
            return Ok(());
        };
        let task_todos = self.session_todos(task.id);

        let project_slug = self.current_project_slug_for_tmux();
        let result = attach_task_with_runtime(
            &self.db,
            project_slug.as_deref(),
            &task,
            &repo,
            &task_todos,
            &self.theme,
            &RealRecoveryRuntime,
        )?;
        match result {
            AttachTaskResult::Attached => {
                self.active_dialog = ActiveDialog::None;
                self.refresh_data()?;
            }
            AttachTaskResult::WorktreeNotFound => {
                self.active_dialog = ActiveDialog::WorktreeNotFound(WorktreeNotFoundDialogState {
                    task_id: task.id,
                    task_title: task.title,
                    focused_field: WorktreeNotFoundField::Recreate,
                });
            }
            AttachTaskResult::RepoUnavailable => {
                self.active_dialog = ActiveDialog::RepoUnavailable(RepoUnavailableDialogState {
                    task_title: task.title,
                    repo_path: repo.path,
                });
                self.refresh_data()?;
            }
        }

        Ok(())
    }

    fn recreate_from_repo_root(&mut self) -> Result<()> {
        let task_id = match &self.active_dialog {
            ActiveDialog::WorktreeNotFound(state) => state.task_id,
            _ => return Ok(()),
        };

        let Some(task) = self.tasks.iter().find(|task| task.id == task_id).cloned() else {
            self.active_dialog = ActiveDialog::None;
            return Ok(());
        };

        let Some(repo) = self.repo_for_task(&task) else {
            self.active_dialog = ActiveDialog::None;
            return Ok(());
        };

        if !Path::new(&repo.path).exists() {
            self.active_dialog = ActiveDialog::RepoUnavailable(RepoUnavailableDialogState {
                task_title: task.title,
                repo_path: repo.path,
            });
            return Ok(());
        }

        self.db.update_task_tmux(task.id, None, Some(repo.path))?;
        self.db.update_task_status(task.id, Status::Idle.as_str())?;

        self.active_dialog = ActiveDialog::None;
        self.refresh_data()?;
        self.attach_selected_task()
    }

    fn mark_worktree_missing_as_broken(&mut self) -> Result<()> {
        let task_id = match &self.active_dialog {
            ActiveDialog::WorktreeNotFound(state) => state.task_id,
            _ => return Ok(()),
        };

        self.db.update_task_status(task_id, Status::Idle.as_str())?;
        self.active_dialog = ActiveDialog::None;
        self.refresh_data()
    }

    fn confirm_new_task(&mut self) -> Result<()> {
        let ActiveDialog::NewTask(mut dialog_state) = self.active_dialog.clone() else {
            return Ok(());
        };

        dialog_state.loading_message = Some(if dialog_state.use_existing_directory {
            "Creating task from existing directory...".to_string()
        } else {
            "Fetching git refs and creating task...".to_string()
        });
        self.active_dialog = ActiveDialog::NewTask(dialog_state.clone());

        let todo_category = self
            .categories
            .iter()
            .find(|category| category.slug == "todo")
            .or_else(|| self.categories.first())
            .map(|category| category.id)
            .context("no category available for new task")?;

        let project_slug = self.current_project_slug_for_tmux();
        let result = create_task_pipeline_with_runtime(
            &self.db,
            &mut self.repos,
            todo_category,
            &dialog_state,
            project_slug.as_deref(),
            &RealCreateTaskRuntime,
        );

        match result {
            Ok(outcome) => {
                self.footer_notice = outcome.warning;
                self.active_dialog = ActiveDialog::None;
                self.refresh_data()?;
            }
            Err(err) => {
                self.active_dialog = ActiveDialog::Error(create_task_error_dialog_state(&err));
            }
        }

        Ok(())
    }

    fn maybe_show_tmux_mouse_hint(&mut self) {
        if self.mouse_hint_shown || self.mouse_seen {
            return;
        }
        if self.started_at.elapsed() >= Duration::from_secs(10) {
            self.footer_notice = Some(
                " tmux mouse hint: run `tmux set -g mouse on` for click+scroll support "
                    .to_string(),
            );
            self.mouse_hint_shown = true;
        }
    }
}

fn default_view_mode(settings: &crate::settings::Settings) -> ViewMode {
    if settings.default_view == "detail" {
        ViewMode::SidePanel
    } else {
        ViewMode::Kanban
    }
}

fn palette_index_for(current: Option<&str>) -> usize {
    let normalized_current = normalize_category_color_key(current);
    CATEGORY_COLOR_PALETTE
        .iter()
        .position(|candidate| *candidate == normalized_current)
        .unwrap_or(0)
}

fn next_palette_color(current: Option<&str>) -> Option<String> {
    let next_idx = (palette_index_for(current) + 1) % CATEGORY_COLOR_PALETTE.len();
    CATEGORY_COLOR_PALETTE[next_idx].map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::interaction::InteractionLayer;
    use super::log::format_numeric_timestamp;
    use super::side_panel::{
        selected_task_from_side_panel_rows, side_panel_rows_from, sorted_categories_with_indexes,
    };
    use super::workflows::{
        build_attach_popup_lines, parse_existing_branch_name, popup_style_from_theme,
        reconcile_startup_tasks, repo_match_candidates, repo_selection_command_id,
        repo_selection_usage_map, resolve_repo_for_creation, tmux_hex_color,
    };
    use super::*;

    use std::cell::{Cell, RefCell};
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;
    use std::time::Instant;

    use chrono::Utc;
    use crossterm::event::{
        KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
    };
    use tempfile::TempDir;
    use tuirealm::ratatui::widgets::ListState;

    use crate::keybindings::{KeyAction, KeyContext, Keybindings};
    use crate::opencode::OpenCodeServerManager;
    use crate::tmux::PopupThemeStyle;
    use crate::types::CommandFrequency;

    fn test_category(id: Uuid, name: &str, position: i64) -> Category {
        let slug = name
            .to_ascii_lowercase()
            .replace(' ', "-")
            .replace('_', "-");
        Category {
            id,
            slug,
            name: name.to_string(),
            position,
            color: None,
            created_at: "now".to_string(),
        }
    }

    fn test_task(category_id: Uuid, position: i64, title: &str) -> Task {
        Task {
            id: Uuid::new_v4(),
            title: title.to_string(),
            repo_id: Uuid::new_v4(),
            branch: "feature/test".to_string(),
            category_id,
            position,
            tmux_session_name: Some(title.to_string()),
            worktree_path: None,
            tmux_status: "idle".to_string(),
            status_source: "none".to_string(),
            status_fetched_at: None,
            status_error: None,
            opencode_session_id: None,
            attach_overlay_shown: false,
            archived: false,
            archived_at: None,
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
        }
    }

    fn key_char(ch: char) -> KeyEvent {
        let modifiers = if ch.is_ascii_uppercase() {
            KeyModifiers::SHIFT
        } else {
            KeyModifiers::empty()
        };
        KeyEvent::new(KeyCode::Char(ch), modifiers)
    }

    fn key_ctrl_char(ch: char) -> KeyEvent {
        KeyEvent::new(
            KeyCode::Char(ch.to_ascii_lowercase()),
            KeyModifiers::CONTROL,
        )
    }

    fn key_enter() -> KeyEvent {
        KeyEvent::new(KeyCode::Enter, KeyModifiers::empty())
    }

    fn test_repo(name: &str, path: &str) -> Repo {
        Repo {
            id: Uuid::new_v4(),
            path: path.to_string(),
            name: name.to_string(),
            default_base: Some("main".to_string()),
            remote_url: None,
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
        }
    }

    struct RecordingRecoveryRuntime {
        repo_exists: bool,
        worktree_exists: bool,
        session_exists: bool,
        create_session_calls: Cell<usize>,
        switch_client_calls: Cell<usize>,
        show_attach_popup_calls: Cell<usize>,
        switched_session_names: RefCell<Vec<String>>,
    }

    impl RecordingRecoveryRuntime {
        fn with_session_exists(session_exists: bool) -> Self {
            Self {
                repo_exists: true,
                worktree_exists: true,
                session_exists,
                create_session_calls: Cell::new(0),
                switch_client_calls: Cell::new(0),
                show_attach_popup_calls: Cell::new(0),
                switched_session_names: RefCell::new(Vec::new()),
            }
        }
    }

    impl RecoveryRuntime for RecordingRecoveryRuntime {
        fn repo_exists(&self, _path: &Path) -> bool {
            self.repo_exists
        }

        fn worktree_exists(&self, _worktree_path: &Path) -> bool {
            self.worktree_exists
        }

        fn session_exists(&self, _session_name: &str) -> bool {
            self.session_exists
        }

        fn create_session(
            &self,
            _session_name: &str,
            _working_dir: &Path,
            _command: &str,
        ) -> Result<()> {
            self.create_session_calls
                .set(self.create_session_calls.get() + 1);
            Ok(())
        }

        fn switch_client(
            &self,
            session_name: &str,
            _reopen_lines: &[String],
            _style: &PopupThemeStyle,
        ) -> Result<()> {
            self.switch_client_calls
                .set(self.switch_client_calls.get() + 1);
            self.switched_session_names
                .borrow_mut()
                .push(session_name.to_string());
            Ok(())
        }

        fn show_attach_popup(&self, _lines: &[String], _style: &PopupThemeStyle) -> Result<()> {
            self.show_attach_popup_calls
                .set(self.show_attach_popup_calls.get() + 1);
            Ok(())
        }
    }

    #[test]
    fn build_attach_popup_lines_contains_task_context_and_navigation() {
        let mut task = test_task(Uuid::new_v4(), 0, "add popup overlay");
        task.branch = "feat/tmux-overlay".to_string();
        task.worktree_path = Some("/tmp/worktrees/feat-tmux-overlay".to_string());
        let repo = test_repo("opencode-kanban", "/tmp/opencode-kanban");
        let todos = vec![
            SessionTodoItem {
                content: "done".to_string(),
                completed: true,
            },
            SessionTodoItem {
                content: "active".to_string(),
                completed: false,
            },
            SessionTodoItem {
                content: "pending".to_string(),
                completed: false,
            },
        ];

        let lines = build_attach_popup_lines(&task, &repo, "ok-opencode-feat-tmux-overlay", &todos);

        assert_eq!(lines[0], "Task attached");
        assert!(lines.iter().any(|line| line.contains("add popup overlay")));
        assert!(lines.iter().any(|line| line.contains("opencode-kanban")));
        assert!(lines.iter().any(|line| line.contains("feat/tmux-overlay")));
        assert!(
            lines
                .iter()
                .any(|line| line.contains("ok-opencode-feat-tmux-overlay"))
        );
        assert!(lines.iter().any(|line| line.contains("Prefix+K")));
        assert!(lines.iter().any(|line| line.contains("Prefix+O")));
        assert!(lines.iter().any(|line| line == "Todo list"));
        assert!(lines.iter().any(|line| line == "[x] done"));
        assert!(lines.iter().any(|line| line == "[>] active"));
        assert!(lines.iter().any(|line| line == "[ ] pending"));
    }

    #[test]
    fn attach_task_with_existing_session_shows_attach_popup_overlay_once() -> Result<()> {
        let db = Database::open(":memory:")?;
        let mut task = test_task(Uuid::new_v4(), 0, "existing-session");
        task.tmux_session_name = Some("ok-existing-session".to_string());
        task.attach_overlay_shown = false;
        let repo = test_repo("repo", "/tmp/repo");
        let runtime = RecordingRecoveryRuntime::with_session_exists(true);
        let result =
            attach_task_with_runtime(&db, None, &task, &repo, &[], &Theme::default(), &runtime)?;

        assert_eq!(result, AttachTaskResult::Attached);
        assert_eq!(runtime.create_session_calls.get(), 0);
        assert_eq!(runtime.switch_client_calls.get(), 1);
        assert_eq!(runtime.show_attach_popup_calls.get(), 1);
        assert_eq!(
            runtime.switched_session_names.borrow().as_slice(),
            ["ok-existing-session"]
        );

        Ok(())
    }

    #[test]
    fn attach_task_with_existing_session_skips_popup_after_first_show() -> Result<()> {
        let db = Database::open(":memory:")?;
        let mut task = test_task(Uuid::new_v4(), 0, "existing-session");
        task.tmux_session_name = Some("ok-existing-session".to_string());
        task.attach_overlay_shown = true;
        let repo = test_repo("repo", "/tmp/repo");
        let runtime = RecordingRecoveryRuntime::with_session_exists(true);
        let result =
            attach_task_with_runtime(&db, None, &task, &repo, &[], &Theme::default(), &runtime)?;

        assert_eq!(result, AttachTaskResult::Attached);
        assert_eq!(runtime.create_session_calls.get(), 0);
        assert_eq!(runtime.switch_client_calls.get(), 1);
        assert_eq!(runtime.show_attach_popup_calls.get(), 0);
        assert_eq!(
            runtime.switched_session_names.borrow().as_slice(),
            ["ok-existing-session"]
        );

        Ok(())
    }

    #[test]
    fn popup_style_from_theme_uses_theme_palette_colors() {
        let theme = Theme::default();
        let style = popup_style_from_theme(&theme);

        assert!(style.popup_style.starts_with("fg=#"));
        assert!(style.popup_style.contains(",bg=#"));
        assert!(style.border_style.starts_with("fg=#"));
        assert!(style.border_style.contains(",bg=#"));
    }

    #[test]
    fn rank_repos_for_query_matches_folder_segments() {
        let repos = vec![
            test_repo("frontend-app", "/work/acme/frontend-app"),
            test_repo("backend-api", "/work/acme/backend-api"),
        ];

        let ranked = rank_repos_for_query("backend", &repos, &HashMap::new());
        assert_eq!(ranked.first().copied(), Some(1));

        let ranked = rank_repos_for_query("acme/frontend", &repos, &HashMap::new());
        assert_eq!(ranked.first().copied(), Some(0));
    }

    #[test]
    fn rank_repos_for_query_empty_prefers_recent_selection_history() {
        let repos = vec![
            test_repo("frontend-app", "/work/acme/frontend-app"),
            test_repo("backend-api", "/work/acme/backend-api"),
        ];

        let mut usage = HashMap::new();
        usage.insert(
            repos[1].id,
            CommandFrequency {
                command_id: repo_selection_command_id(repos[1].id),
                use_count: 10,
                last_used: (Utc::now() - chrono::Duration::hours(1)).to_rfc3339(),
            },
        );

        let ranked = rank_repos_for_query("", &repos, &usage);
        assert_eq!(ranked.first().copied(), Some(1));
    }

    #[test]
    fn parse_existing_branch_name_detects_git_branch_collision() {
        let detail =
            "stderr: Preparing worktree (new branch 'c')\nfatal: a branch named 'c' already exists";
        assert_eq!(parse_existing_branch_name(detail), Some("c".to_string()));
    }

    #[test]
    fn create_task_error_dialog_state_branch_collision_is_concise() {
        let err = anyhow::anyhow!(
            "worktree creation failed: failed to create worktree `/home/cc/.opencode-kanban-worktrees/test/c-2` for branch `c` from `main`: git command failed in /home/cc/codes/playgrounds/test: git worktree add -b c /home/cc/.opencode-kanban-worktrees/test/c-2 main\nstdout:\nstderr: Preparing worktree (new branch 'c')\nfatal: a branch named 'c' already exists"
        );

        let dialog = create_task_error_dialog_state(&err);
        assert_eq!(dialog.title, "Branch already exists");
        assert!(dialog.detail.contains("Branch `c` already exists"));
        assert!(!dialog.detail.contains("git worktree add -b"));
    }

    #[test]
    fn resolve_repo_for_creation_accepts_fuzzy_existing_repo_query() -> Result<()> {
        let db = Database::open(":memory:")?;
        let temp = TempDir::new()?;
        let frontend = temp.path().join("acme").join("frontend-app");
        let backend = temp.path().join("acme").join("backend-api");
        fs::create_dir_all(&frontend)?;
        fs::create_dir_all(&backend)?;

        let _frontend_repo = db.add_repo(&frontend)?;
        let backend_repo = db.add_repo(&backend)?;
        db.increment_command_usage(&repo_selection_command_id(backend_repo.id))?;
        db.increment_command_usage(&repo_selection_command_id(backend_repo.id))?;

        let mut repos = db.list_repos()?;
        let state = NewTaskDialogState {
            repo_idx: 0,
            repo_input: "backend".to_string(),
            repo_picker: None,
            use_existing_directory: false,
            existing_dir_input: String::new(),
            branch_input: String::new(),
            base_input: String::new(),
            title_input: String::new(),
            ensure_base_up_to_date: true,
            loading_message: None,
            focused_field: NewTaskField::Repo,
        };

        let runtime = RealCreateTaskRuntime;
        let selected = resolve_repo_for_creation(&db, &mut repos, &state, &runtime)?;
        assert_eq!(selected.id, backend_repo.id);
        Ok(())
    }

    fn mouse_event(kind: MouseEventKind, column: u16, row: u16) -> MouseEvent {
        MouseEvent {
            kind,
            column,
            row,
            modifiers: KeyModifiers::empty(),
        }
    }
    fn category_positions(app: &App) -> Vec<(Uuid, i64)> {
        app.categories
            .iter()
            .map(|category| (category.id, category.position))
            .collect()
    }

    fn test_app_with_middle_task() -> Result<(App, TempDir, Uuid, [Uuid; 3])> {
        let db = Database::open(":memory:")?;
        let repo_dir = TempDir::new()?;
        let repo = db.add_repo(repo_dir.path())?;
        let categories = db.list_categories()?;
        let ids = [categories[0].id, categories[1].id, categories[2].id];
        let task = db.add_task(repo.id, "feature/category-edit-tests", "Task", ids[1])?;

        let mut app = App {
            should_quit: false,
            pulse_phase: 0,
            theme: Theme::default(),
            layout_epoch: 0,
            viewport: (120, 40),
            last_mouse_event: None,
            db,
            tasks: Vec::new(),
            categories: Vec::new(),
            repos: Vec::new(),
            archived_tasks: Vec::new(),
            focused_column: 0,
            selected_task_per_column: HashMap::new(),
            scroll_offset_per_column: HashMap::new(),
            column_scroll_states: Vec::new(),
            active_dialog: ActiveDialog::None,
            footer_notice: None,
            interaction_map: InteractionMap::default(),
            hovered_message: None,
            context_menu: None,
            current_view: View::Board,
            current_project_path: None,
            project_list: Vec::new(),
            selected_project_index: 0,
            project_list_state: ListState::default(),
            started_at: Instant::now(),
            mouse_seen: false,
            mouse_hint_shown: false,
            _server_manager: OpenCodeServerManager::new(),
            poller_stop: Arc::new(AtomicBool::new(false)),
            poller_thread: None,
            view_mode: ViewMode::Kanban,
            side_panel_width: 40,
            side_panel_selected_row: 0,
            archive_selected_index: 0,
            collapsed_categories: HashSet::new(),
            current_log_buffer: None,
            current_change_summary: None,
            detail_focus: DetailFocus::List,
            detail_scroll_offset: 0,
            log_scroll_offset: 0,
            log_split_ratio: 65,
            log_expanded: false,
            log_expanded_scroll_offset: 0,
            log_expanded_entries: HashSet::new(),
            session_todo_cache: Arc::new(Mutex::new(HashMap::new())),
            session_subagent_cache: Arc::new(Mutex::new(HashMap::new())),
            session_title_cache: Arc::new(Mutex::new(HashMap::new())),
            session_message_cache: Arc::new(Mutex::new(HashMap::new())),
            todo_visualization_mode: TodoVisualizationMode::Checklist,
            keybindings: Keybindings::load(),
            settings: crate::settings::Settings::load(),
            settings_view_state: None,
            category_edit_mode: false,
            project_detail_cache: None,
            last_click: None,
            pending_gg_at: None,
        };

        app.refresh_data()?;
        app.focused_column = 1;
        app.selected_task_per_column.insert(1, 0);

        Ok((app, repo_dir, task.id, ids))
    }

    #[test]
    fn toggle_category_edit_mode_with_ctrl_g_key() -> Result<()> {
        let (mut app, _repo_dir, _task_id, _category_ids) = test_app_with_middle_task()?;

        assert!(!app.category_edit_mode);

        app.handle_key(key_char('g'))?;
        assert!(!app.category_edit_mode);

        app.handle_key(key_ctrl_char('g'))?;
        assert!(app.category_edit_mode);

        app.handle_key(key_ctrl_char('g'))?;
        assert!(!app.category_edit_mode);

        Ok(())
    }

    #[test]
    fn vim_half_page_navigation_ctrl_d_and_ctrl_u_in_kanban() -> Result<()> {
        let (mut app, _repo_dir, _task_id, category_ids) = test_app_with_middle_task()?;
        let repo_id = app.repos[0].id;
        for idx in 0..7 {
            app.db.add_task(
                repo_id,
                &format!("feature/half-page-{idx}"),
                &format!("Half Page {idx}"),
                category_ids[1],
            )?;
        }
        app.refresh_data()?;
        app.focused_column = 1;
        app.selected_task_per_column.insert(1, 0);

        let step = app.board_half_page_step();
        app.handle_key(key_ctrl_char('d'))?;
        assert_eq!(app.selected_task_per_column.get(&1).copied(), Some(step));

        app.handle_key(key_ctrl_char('u'))?;
        assert_eq!(app.selected_task_per_column.get(&1).copied(), Some(0));

        Ok(())
    }

    #[test]
    fn vim_g_and_gg_jump_to_bottom_and_top() -> Result<()> {
        let (mut app, _repo_dir, _task_id, category_ids) = test_app_with_middle_task()?;
        let repo_id = app.repos[0].id;
        for idx in 0..4 {
            app.db.add_task(
                repo_id,
                &format!("feature/g-jump-{idx}"),
                &format!("Jump {idx}"),
                category_ids[1],
            )?;
        }
        app.refresh_data()?;
        app.focused_column = 1;
        app.selected_task_per_column.insert(1, 0);

        let max_index = app.tasks_in_column(1).saturating_sub(1);

        app.handle_key(key_char('G'))?;
        assert_eq!(
            app.selected_task_per_column.get(&1).copied(),
            Some(max_index)
        );

        app.handle_key(key_char('g'))?;
        assert_eq!(
            app.selected_task_per_column.get(&1).copied(),
            Some(max_index)
        );

        app.handle_key(key_char('g'))?;
        assert_eq!(app.selected_task_per_column.get(&1).copied(), Some(0));

        Ok(())
    }

    #[test]
    fn default_view_setting_maps_to_kanban_mode() {
        let settings = crate::settings::Settings {
            default_view: "kanban".to_string(),
            ..crate::settings::Settings::default()
        };

        assert_eq!(default_view_mode(&settings), ViewMode::Kanban);
    }

    #[test]
    fn default_view_setting_maps_to_detail_mode() {
        let settings = crate::settings::Settings {
            default_view: "detail".to_string(),
            ..crate::settings::Settings::default()
        };

        assert_eq!(default_view_mode(&settings), ViewMode::SidePanel);
    }

    #[test]
    fn shift_h_and_l_are_mode_scoped_between_task_move_and_category_reorder() -> Result<()> {
        let (mut app, _repo_dir, task_id, [todo_id, in_progress_id, done_id]) =
            test_app_with_middle_task()?;

        app.category_edit_mode = false;
        app.focused_column = 1;
        app.selected_task_per_column.insert(1, 0);
        app.handle_key(key_char('H'))?;

        let moved_task = app.db.get_task(task_id)?;
        assert_eq!(moved_task.category_id, todo_id);

        app.db.update_task_category(task_id, in_progress_id, 0)?;
        app.refresh_data()?;
        app.focused_column = 1;
        app.selected_task_per_column.insert(1, 0);
        app.category_edit_mode = true;

        app.handle_key(key_char('H'))?;

        let unmoved_task = app.db.get_task(task_id)?;
        assert_eq!(unmoved_task.category_id, in_progress_id);

        let after_left = category_positions(&app);
        assert_eq!(
            after_left,
            vec![(in_progress_id, 0), (todo_id, 1), (done_id, 2)]
        );

        app.handle_key(key_char('L'))?;

        let after_right = category_positions(&app);
        assert_eq!(
            after_right,
            vec![(todo_id, 0), (in_progress_id, 1), (done_id, 2)]
        );

        Ok(())
    }

    #[test]
    fn category_reorder_keys_noop_at_left_and_right_boundaries() -> Result<()> {
        let (mut app, _repo_dir, _task_id, _category_ids) = test_app_with_middle_task()?;
        app.category_edit_mode = true;

        app.focused_column = 0;
        let left_before = category_positions(&app);
        app.handle_key(key_char('H'))?;
        assert_eq!(category_positions(&app), left_before);

        app.focused_column = app.categories.len() - 1;
        let right_before = category_positions(&app);
        app.handle_key(key_char('L'))?;
        assert_eq!(category_positions(&app), right_before);

        Ok(())
    }

    #[test]
    fn category_color_dialog_enter_confirms_or_cancels_based_on_focus() -> Result<()> {
        let (mut app, _repo_dir, _task_id, [_todo_id, in_progress_id, _done_id]) =
            test_app_with_middle_task()?;
        app.focused_column = 1;
        app.category_edit_mode = true;

        app.handle_key(key_char('p'))?;
        match &mut app.active_dialog {
            ActiveDialog::CategoryColor(state) => {
                state.selected_index = 1;
                state.focused_field = CategoryColorField::Confirm;
            }
            _ => panic!("expected category color dialog to open"),
        }
        app.handle_key(key_enter())?;

        assert_eq!(app.active_dialog, ActiveDialog::None);
        let categories_after_confirm = app.db.list_categories()?;
        let confirmed_color = categories_after_confirm
            .iter()
            .find(|category| category.id == in_progress_id)
            .and_then(|category| category.color.as_deref());
        assert_eq!(confirmed_color, Some("primary"));

        app.handle_key(key_char('p'))?;
        match &mut app.active_dialog {
            ActiveDialog::CategoryColor(state) => {
                state.selected_index = 6;
                state.focused_field = CategoryColorField::Cancel;
            }
            _ => panic!("expected category color dialog to open"),
        }
        app.handle_key(key_enter())?;

        assert_eq!(app.active_dialog, ActiveDialog::None);
        let categories_after_cancel = app.db.list_categories()?;
        let canceled_color = categories_after_cancel
            .iter()
            .find(|category| category.id == in_progress_id)
            .and_then(|category| category.color.as_deref());
        assert_eq!(canceled_color, Some("primary"));

        Ok(())
    }

    #[test]
    fn settings_category_color_toggle_updates_selected_category() -> Result<()> {
        let (mut app, _repo_dir, _task_id, [_todo_id, in_progress_id, _done_id]) =
            test_app_with_middle_task()?;

        app.focused_column = 1;
        app.update(Message::OpenSettings)?;
        if let Some(state) = &mut app.settings_view_state {
            state.active_section = SettingsSection::CategoryColors;
            state.category_color_selected = 1;
        }

        app.update(Message::SettingsToggle)?;

        let categories_after_toggle = app.db.list_categories()?;
        let toggled_color = categories_after_toggle
            .iter()
            .find(|category| category.id == in_progress_id)
            .and_then(|category| category.color.as_deref());
        assert_eq!(toggled_color, Some("primary"));

        Ok(())
    }

    #[test]
    fn settings_category_color_selection_moves_with_j_and_k() -> Result<()> {
        let (mut app, _repo_dir, _task_id, _category_ids) = test_app_with_middle_task()?;

        app.update(Message::OpenSettings)?;
        if let Some(state) = &mut app.settings_view_state {
            state.active_section = SettingsSection::CategoryColors;
            state.category_color_selected = 0;
        }

        app.handle_key(key_char('j'))?;
        assert_eq!(
            app.settings_view_state
                .as_ref()
                .map(|state| state.category_color_selected),
            Some(1)
        );

        app.handle_key(key_char('k'))?;
        assert_eq!(
            app.settings_view_state
                .as_ref()
                .map(|state| state.category_color_selected),
            Some(0)
        );

        Ok(())
    }

    #[test]
    fn mouse_left_click_selects_task_from_interaction_map() -> Result<()> {
        let (mut app, _repo_dir, _task_id, _category_ids) = test_app_with_middle_task()?;
        app.focused_column = 0;
        app.interaction_map.register_task(
            InteractionLayer::Base,
            Rect::new(10, 5, 20, 5),
            Message::SelectTask(1, 0),
        );

        app.handle_mouse(mouse_event(MouseEventKind::Down(MouseButton::Left), 12, 6))?;

        assert_eq!(app.focused_column, 1);
        assert_eq!(app.selected_task_per_column.get(&1).copied(), Some(0));
        Ok(())
    }

    #[test]
    fn mouse_scroll_down_moves_board_column_offset() -> Result<()> {
        let (mut app, _repo_dir, _task_id, category_ids) = test_app_with_middle_task()?;
        let repo_id = app.repos[0].id;
        app.db
            .add_task(repo_id, "feature/scroll-1", "Scroll 1", category_ids[1])?;
        app.db
            .add_task(repo_id, "feature/scroll-2", "Scroll 2", category_ids[1])?;
        app.refresh_data()?;
        app.focused_column = 1;

        app.handle_mouse(mouse_event(MouseEventKind::ScrollDown, 20, 10))?;

        assert_eq!(app.clamped_scroll_offset_for_column(1), 1);
        Ok(())
    }

    #[test]
    fn mouse_right_click_opens_context_menu_for_task() -> Result<()> {
        let (mut app, _repo_dir, task_id, _category_ids) = test_app_with_middle_task()?;
        app.interaction_map.register_task(
            InteractionLayer::Base,
            Rect::new(10, 5, 20, 5),
            Message::SelectTask(1, 0),
        );

        app.handle_mouse(mouse_event(MouseEventKind::Down(MouseButton::Right), 12, 6))?;

        assert!(app.context_menu.is_some());
        let menu = app
            .context_menu
            .as_ref()
            .expect("context menu should exist");
        assert_eq!(menu.task_column, 1);
        assert_eq!(menu.task_id, task_id);
        Ok(())
    }

    #[test]
    fn mouse_click_selects_side_panel_row() -> Result<()> {
        let (mut app, _repo_dir, _task_id, _category_ids) = test_app_with_middle_task()?;
        app.view_mode = ViewMode::SidePanel;
        app.detail_focus = DetailFocus::Details;

        app.interaction_map.register_click(
            InteractionLayer::Base,
            Rect::new(8, 8, 20, 1),
            Message::SelectTaskInSidePanel(2),
        );

        app.handle_mouse(mouse_event(MouseEventKind::Down(MouseButton::Left), 9, 8))?;

        assert_eq!(app.side_panel_selected_row, 2);
        assert_eq!(app.detail_focus, DetailFocus::List);
        Ok(())
    }

    #[test]
    fn mouse_scroll_moves_side_panel_selection_when_in_side_panel_mode() -> Result<()> {
        let (mut app, _repo_dir, _task_id, _category_ids) = test_app_with_middle_task()?;
        app.view_mode = ViewMode::SidePanel;
        app.detail_focus = DetailFocus::List;
        app.side_panel_selected_row = 0;

        app.handle_mouse(mouse_event(MouseEventKind::ScrollDown, 20, 10))?;
        assert_eq!(app.side_panel_selected_row, 1);

        app.handle_mouse(mouse_event(MouseEventKind::ScrollUp, 20, 10))?;
        assert_eq!(app.side_panel_selected_row, 0);
        Ok(())
    }

    #[test]
    fn mouse_scroll_over_side_panel_list_area_forces_list_scroll() -> Result<()> {
        let (mut app, _repo_dir, _task_id, _category_ids) = test_app_with_middle_task()?;
        app.view_mode = ViewMode::SidePanel;
        app.detail_focus = DetailFocus::Details;
        app.side_panel_selected_row = 0;

        app.interaction_map.register_click(
            InteractionLayer::Base,
            Rect::new(4, 4, 30, 10),
            Message::FocusSidePanel(DetailFocus::List),
        );

        app.handle_mouse(mouse_event(MouseEventKind::ScrollDown, 5, 5))?;

        assert_eq!(app.detail_focus, DetailFocus::List);
        assert_eq!(app.side_panel_selected_row, 1);
        Ok(())
    }

    #[test]
    fn mouse_click_focuses_new_task_dialog_input_field() -> Result<()> {
        let (mut app, _repo_dir, _task_id, _category_ids) = test_app_with_middle_task()?;
        app.update(Message::OpenNewTaskDialog)?;

        app.interaction_map.register_click(
            InteractionLayer::Dialog,
            Rect::new(12, 8, 24, 3),
            Message::FocusNewTaskField(NewTaskField::Branch),
        );

        app.handle_mouse(mouse_event(MouseEventKind::Down(MouseButton::Left), 14, 9))?;

        match &app.active_dialog {
            ActiveDialog::NewTask(state) => {
                assert_eq!(state.focused_field, NewTaskField::Branch);
            }
            other => panic!("expected NewTask dialog, got {other:?}"),
        }

        Ok(())
    }

    #[test]
    fn mouse_click_toggles_new_task_checkbox() -> Result<()> {
        let (mut app, _repo_dir, _task_id, _category_ids) = test_app_with_middle_task()?;
        app.update(Message::OpenNewTaskDialog)?;

        app.interaction_map.register_click(
            InteractionLayer::Dialog,
            Rect::new(12, 16, 24, 3),
            Message::ToggleNewTaskCheckbox,
        );

        app.handle_mouse(mouse_event(MouseEventKind::Down(MouseButton::Left), 15, 17))?;

        match &app.active_dialog {
            ActiveDialog::NewTask(state) => {
                assert_eq!(state.focused_field, NewTaskField::EnsureBaseUpToDate);
                assert!(!state.ensure_base_up_to_date);
            }
            other => panic!("expected NewTask dialog, got {other:?}"),
        }

        Ok(())
    }

    #[test]
    fn mouse_click_toggles_delete_task_checkbox() -> Result<()> {
        let (mut app, _repo_dir, _task_id, _category_ids) = test_app_with_middle_task()?;
        app.update(Message::OpenDeleteTaskDialog)?;

        app.interaction_map.register_click(
            InteractionLayer::Dialog,
            Rect::new(12, 12, 10, 3),
            Message::ToggleDeleteTaskCheckbox(DeleteTaskField::KillTmux),
        );

        app.handle_mouse(mouse_event(MouseEventKind::Down(MouseButton::Left), 15, 13))?;

        match &app.active_dialog {
            ActiveDialog::DeleteTask(state) => {
                assert_eq!(state.focused_field, DeleteTaskField::KillTmux);
                assert!(!state.kill_tmux);
            }
            other => panic!("expected DeleteTask dialog, got {other:?}"),
        }

        Ok(())
    }

    #[test]
    fn delete_task_dialog_defaults_to_kill_tmux_only() -> Result<()> {
        let (mut app, _repo_dir, _task_id, _category_ids) = test_app_with_middle_task()?;
        app.update(Message::OpenDeleteTaskDialog)?;

        match &app.active_dialog {
            ActiveDialog::DeleteTask(state) => {
                assert!(state.kill_tmux);
                assert!(!state.remove_worktree);
                assert!(!state.delete_branch);
                assert!(!state.confirm_destructive);
            }
            other => panic!("expected DeleteTask dialog, got {other:?}"),
        }

        Ok(())
    }

    #[test]
    fn delete_task_requires_second_confirm_for_destructive_cleanup() -> Result<()> {
        let (mut app, _repo_dir, task_id, _category_ids) = test_app_with_middle_task()?;
        app.update(Message::OpenDeleteTaskDialog)?;
        app.update(Message::ToggleDeleteTaskCheckbox(
            DeleteTaskField::RemoveWorktree,
        ))?;

        app.update(Message::ConfirmDeleteTask)?;

        match &app.active_dialog {
            ActiveDialog::DeleteTask(state) => {
                assert!(state.remove_worktree);
                assert!(state.confirm_destructive);
                assert_eq!(state.focused_field, DeleteTaskField::Delete);
            }
            other => panic!("expected DeleteTask dialog, got {other:?}"),
        }
        assert!(app.db.get_task(task_id).is_ok());

        app.update(Message::ConfirmDeleteTask)?;

        assert!(matches!(app.active_dialog, ActiveDialog::None));
        assert!(app.db.get_task(task_id).is_err());
        Ok(())
    }

    #[test]
    fn side_panel_rows_are_grouped_by_sorted_category_position() {
        let todo_id = Uuid::new_v4();
        let doing_id = Uuid::new_v4();
        let categories = vec![
            test_category(todo_id, "TODO", 10),
            test_category(doing_id, "DOING", 5),
        ];
        let tasks = vec![
            test_task(todo_id, 0, "todo-1"),
            test_task(doing_id, 0, "doing-1"),
            test_task(todo_id, 1, "todo-2"),
        ];

        let rows = side_panel_rows_from(&categories, &tasks, &HashSet::new());

        assert!(matches!(
            &rows[0],
            SidePanelRow::CategoryHeader { category_id, .. } if *category_id == doing_id
        ));
        assert!(matches!(
            &rows[1],
            SidePanelRow::Task { category_id, .. } if *category_id == doing_id
        ));
        assert!(matches!(
            &rows[2],
            SidePanelRow::CategoryHeader { category_id, .. } if *category_id == todo_id
        ));
        assert!(matches!(
            &rows[3],
            SidePanelRow::Task { category_id, index_in_column, .. }
            if *category_id == todo_id && *index_in_column == 0
        ));
        assert!(matches!(
            &rows[4],
            SidePanelRow::Task { category_id, index_in_column, .. }
            if *category_id == todo_id && *index_in_column == 1
        ));
    }

    #[test]
    fn side_panel_rows_hide_tasks_for_collapsed_categories() {
        let todo_id = Uuid::new_v4();
        let categories = vec![test_category(todo_id, "TODO", 0)];
        let tasks = vec![
            test_task(todo_id, 0, "todo-1"),
            test_task(todo_id, 1, "todo-2"),
        ];
        let collapsed = HashSet::from([todo_id]);

        let rows = side_panel_rows_from(&categories, &tasks, &collapsed);

        assert_eq!(rows.len(), 1);
        assert!(matches!(
            &rows[0],
            SidePanelRow::CategoryHeader {
                category_id,
                total_tasks,
                visible_tasks,
                collapsed,
                ..
            } if *category_id == todo_id && *total_tasks == 2 && *visible_tasks == 0 && *collapsed
        ));
    }

    #[test]
    fn selected_task_from_side_panel_rows_returns_none_for_header() {
        let todo_id = Uuid::new_v4();
        let rows = vec![
            SidePanelRow::CategoryHeader {
                column_index: 0,
                category_id: todo_id,
                category_name: "TODO".to_string(),
                category_color: None,
                total_tasks: 1,
                visible_tasks: 1,
                collapsed: false,
            },
            SidePanelRow::Task {
                column_index: 0,
                index_in_column: 0,
                category_id: todo_id,
                task: Box::new(test_task(todo_id, 0, "todo-1")),
            },
        ];

        assert!(selected_task_from_side_panel_rows(&rows, 0).is_none());
        assert!(
            selected_task_from_side_panel_rows(&rows, 1).is_some(),
            "task row should resolve to selected task"
        );
    }

    #[test]
    fn test_log_kind_label() {
        assert_eq!(log_kind_label(Some("text")), "SAY");
        assert_eq!(log_kind_label(None), "SAY");
        assert_eq!(log_kind_label(Some("tool")), "TOOL");
        assert_eq!(log_kind_label(Some("reasoning")), "THINK");
        assert_eq!(log_kind_label(Some("step-start")), "STEP+");
        assert_eq!(log_kind_label(Some("patch")), "PATCH");
        assert_eq!(log_kind_label(Some("file")), "FILE");
        assert_eq!(log_kind_label(Some("unknown")), "UNKNOWN");
    }

    #[test]
    fn test_log_role_label() {
        assert_eq!(log_role_label(Some("user")), "USER");
        assert_eq!(log_role_label(Some("assistant")), "ASSISTANT");
        assert_eq!(log_role_label(None), "UNKNOWN");
        assert_eq!(log_role_label(Some("system")), "SYSTEM");
    }

    #[test]
    fn test_log_time_label() {
        assert_eq!(log_time_label(None), "--:--:--");
        assert_eq!(log_time_label(Some("2024-01-15T10:30:00Z")), "10:30:00");
        assert_eq!(log_time_label(Some("2024-01-15 10:30:00")), "10:30:00");
        assert_eq!(log_time_label(Some("invalid")), "invalid");
    }

    #[test]
    fn test_format_numeric_timestamp() {
        assert!(format_numeric_timestamp("1705315800").is_some());
        assert!(format_numeric_timestamp("1705315800.123").is_some());
        assert!(format_numeric_timestamp("invalid").is_none());
        assert!(format_numeric_timestamp("").is_none());
    }

    #[test]
    fn test_palette_index_for() {
        assert_eq!(palette_index_for(None), 0);
        assert_eq!(palette_index_for(Some("primary")), 1);
        assert_eq!(palette_index_for(Some("secondary")), 2);
        assert_eq!(palette_index_for(Some("tertiary")), 3);
        assert_eq!(palette_index_for(Some("success")), 4);
        assert_eq!(palette_index_for(Some("warning")), 5);
        assert_eq!(palette_index_for(Some("danger")), 6);
        assert_eq!(palette_index_for(Some("unknown")), 0);
    }

    #[test]
    fn test_next_palette_color() {
        assert_eq!(next_palette_color(None), Some("primary".to_string()));
        assert_eq!(
            next_palette_color(Some("primary")),
            Some("secondary".to_string())
        );
        assert_eq!(next_palette_color(Some("danger")), None);
    }

    #[test]
    fn test_sorted_categories_with_indexes() {
        let cat_a = test_category(Uuid::new_v4(), "A", 0);
        let cat_b = test_category(Uuid::new_v4(), "B", 1);
        let cat_c = test_category(Uuid::new_v4(), "C", 2);
        let categories = vec![cat_c.clone(), cat_a.clone(), cat_b.clone()];
        let sorted = sorted_categories_with_indexes(&categories);
        assert_eq!(sorted.len(), 3);
        assert_eq!(sorted[0].1.position, 0);
        assert_eq!(sorted[1].1.position, 1);
        assert_eq!(sorted[2].1.position, 2);
    }

    #[test]
    fn test_default_view_mode() {
        let kanban_settings = crate::settings::Settings {
            default_view: "kanban".to_string(),
            ..crate::settings::Settings::default()
        };
        assert_eq!(default_view_mode(&kanban_settings), ViewMode::Kanban);

        let detail_settings = crate::settings::Settings {
            default_view: "detail".to_string(),
            ..crate::settings::Settings::default()
        };
        assert_eq!(default_view_mode(&detail_settings), ViewMode::SidePanel);

        let unknown_settings = crate::settings::Settings {
            default_view: "unknown".to_string(),
            ..crate::settings::Settings::default()
        };
        assert_eq!(default_view_mode(&unknown_settings), ViewMode::Kanban);
    }

    #[test]
    fn test_tmux_hex_color() {
        use tuirealm::ratatui::style::Color;
        assert_eq!(tmux_hex_color(Color::Rgb(255, 0, 0)), "#ff0000");
        assert_eq!(tmux_hex_color(Color::Black), "#000000");
        assert_eq!(tmux_hex_color(Color::White), "#ffffff");
    }

    #[test]
    fn test_point_in_rect() {
        use tuirealm::ratatui::layout::Rect;
        let rect = Rect::new(10, 20, 30, 40);
        assert!(point_in_rect(15, 25, rect));
        assert!(point_in_rect(10, 20, rect));
        assert!(!point_in_rect(5, 25, rect));
        assert!(!point_in_rect(45, 25, rect));
        assert!(!point_in_rect(15, 15, rect));
        assert!(!point_in_rect(15, 65, rect));
    }

    #[test]
    fn test_repo_selection_command_id() {
        let id = Uuid::new_v4();
        let cmd_id = repo_selection_command_id(id);
        assert!(cmd_id.contains(&id.to_string()));
    }

    #[test]
    fn test_repo_match_candidates() {
        let repo = test_repo("my-project", "/work/company/my-project");
        let candidates = repo_match_candidates(&repo);
        assert!(!candidates.is_empty());
        assert!(candidates.iter().any(|(s, _)| s == "my-project"));
        assert!(candidates.iter().any(|(s, _)| s.contains("company")));
    }

    fn find_toggle_help_key(app: &App) -> KeyEvent {
        let candidates = [
            KeyEvent::new(KeyCode::Char('?'), KeyModifiers::empty()),
            KeyEvent::new(KeyCode::Char('?'), KeyModifiers::SHIFT),
            KeyEvent::new(KeyCode::F(1), KeyModifiers::empty()),
            KeyEvent::new(KeyCode::Char('h'), KeyModifiers::CONTROL),
        ];

        candidates
            .into_iter()
            .find(|key| {
                app.keybindings.action_for_key(KeyContext::Global, *key)
                    == Some(KeyAction::ToggleHelp)
            })
            .expect("toggle-help keybinding should exist")
    }

    #[test]
    fn test_log_kind_label_additional_variants_and_empty_string() {
        assert_eq!(log_kind_label(Some("step-finish")), "STEP-");
        assert_eq!(log_kind_label(Some("subtask")), "SUBTASK");
        assert_eq!(log_kind_label(Some("agent")), "AGENT");
        assert_eq!(log_kind_label(Some("snapshot")), "SNAP");
        assert_eq!(log_kind_label(Some("retry")), "RETRY");
        assert_eq!(log_kind_label(Some("compaction")), "COMPACT");
        assert_eq!(log_kind_label(Some("   ")), "TEXT");
    }

    #[test]
    fn test_format_numeric_timestamp_supports_millis_micros_and_nanos() {
        assert!(format_numeric_timestamp("1705315800123").is_some());
        assert!(format_numeric_timestamp("1705315800123456").is_some());
        assert!(format_numeric_timestamp("1705315800123456789").is_some());
    }

    #[test]
    fn create_task_error_dialog_state_uses_contextual_titles() {
        let worktree = anyhow::anyhow!("worktree creation failed: cannot create");
        let tmux = anyhow::anyhow!("tmux session creation failed: cannot start");
        let generic = anyhow::anyhow!("unexpected failure");

        assert_eq!(
            create_task_error_dialog_state(&worktree).title,
            "Worktree creation failed"
        );
        assert_eq!(
            create_task_error_dialog_state(&tmux).title,
            "Tmux session failed"
        );
        assert_eq!(
            create_task_error_dialog_state(&generic).title,
            "Task creation failed"
        );
    }

    #[test]
    fn repo_selection_usage_map_ignores_invalid_command_ids() -> Result<()> {
        let db = Database::open(":memory:")?;
        let valid_repo_id = Uuid::new_v4();
        db.increment_command_usage(&repo_selection_command_id(valid_repo_id))?;
        db.increment_command_usage("repo-selection:not-a-uuid")?;
        db.increment_command_usage("not-repo-selection")?;

        let usage = repo_selection_usage_map(&db);
        assert_eq!(usage.len(), 1);
        assert!(usage.contains_key(&valid_repo_id));
        Ok(())
    }

    #[test]
    fn resolve_repo_for_creation_rejects_missing_or_non_git_paths() -> Result<()> {
        let db = Database::open(":memory:")?;
        let mut repos = Vec::new();
        let runtime = RealCreateTaskRuntime;

        let missing =
            std::env::temp_dir().join(format!("opencode-kanban-missing-{}", Uuid::new_v4()));
        let missing_state = NewTaskDialogState {
            repo_idx: 0,
            repo_input: missing.display().to_string(),
            repo_picker: None,
            use_existing_directory: false,
            existing_dir_input: String::new(),
            branch_input: String::new(),
            base_input: String::new(),
            title_input: String::new(),
            ensure_base_up_to_date: true,
            loading_message: None,
            focused_field: NewTaskField::Repo,
        };
        let err = resolve_repo_for_creation(&db, &mut repos, &missing_state, &runtime)
            .expect_err("missing path should fail");
        assert!(err.to_string().contains("repo path does not exist"));

        let temp = TempDir::new()?;
        let non_git_state = NewTaskDialogState {
            repo_idx: 0,
            repo_input: temp.path().display().to_string(),
            repo_picker: None,
            use_existing_directory: false,
            existing_dir_input: String::new(),
            branch_input: String::new(),
            base_input: String::new(),
            title_input: String::new(),
            ensure_base_up_to_date: true,
            loading_message: None,
            focused_field: NewTaskField::Repo,
        };
        let err = resolve_repo_for_creation(&db, &mut repos, &non_git_state, &runtime)
            .expect_err("non-git path should fail");
        assert!(err.to_string().contains("not a git repository"));

        Ok(())
    }

    #[test]
    fn load_project_detail_summarizes_running_counts() -> Result<()> {
        let temp = TempDir::new()?;
        let db_path = temp.path().join("project.sqlite");
        let repo_dir = temp.path().join("repo");
        fs::create_dir_all(&repo_dir)?;

        let db = Database::open(&db_path)?;
        let repo = db.add_repo(&repo_dir)?;
        let categories = db.list_categories()?;
        let task = db.add_task(repo.id, "feature/running", "Running", categories[0].id)?;
        db.update_task_status(task.id, Status::Running.as_str())?;

        let info = ProjectInfo {
            name: "demo".to_string(),
            path: db_path,
        };
        let detail = load_project_detail(&info).expect("project detail should be available");
        assert_eq!(detail.project_name, "demo");
        assert_eq!(detail.task_count, 1);
        assert_eq!(detail.running_count, 1);
        assert_eq!(detail.repo_count, 1);
        assert_eq!(detail.category_count, categories.len());
        Ok(())
    }

    #[test]
    fn session_cache_helpers_and_project_slug_behavior() -> Result<()> {
        let (mut app, _repo_dir, task_id, _category_ids) = test_app_with_middle_task()?;

        {
            let mut todos = app.session_todo_cache.lock().expect("todo cache lock");
            todos.insert(
                task_id,
                vec![
                    SessionTodoItem {
                        content: "done".to_string(),
                        completed: true,
                    },
                    SessionTodoItem {
                        content: "next".to_string(),
                        completed: false,
                    },
                ],
            );
        }
        {
            let mut subagents = app
                .session_subagent_cache
                .lock()
                .expect("subagent cache lock");
            subagents.insert(
                task_id,
                vec![SubagentTodoSummary {
                    title: "agent-a".to_string(),
                    todo_summary: Some((1, 2)),
                }],
            );
        }
        {
            let mut titles = app.session_title_cache.lock().expect("title cache lock");
            titles.insert("session-1".to_string(), "Session One".to_string());
        }
        {
            let mut messages = app
                .session_message_cache
                .lock()
                .expect("message cache lock");
            messages.insert(
                task_id,
                vec![SessionMessageItem {
                    role: Some("assistant".to_string()),
                    content: "hello".to_string(),
                    timestamp: Some("2024-01-01T10:00:00Z".to_string()),
                    message_type: Some("text".to_string()),
                }],
            );
        }

        assert_eq!(app.session_todo_summary(task_id), Some((1, 2)));
        assert_eq!(app.session_subagent_summaries(task_id).len(), 1);
        assert_eq!(
            app.opencode_session_title("session-1"),
            Some("Session One".to_string())
        );
        assert_eq!(app.session_messages(task_id).len(), 1);

        app.current_project_path = None;
        assert_eq!(
            app.poller_db_path(),
            projects::get_project_path(projects::DEFAULT_PROJECT)
        );
        assert_eq!(app.current_project_slug_for_tmux(), None);

        app.current_project_path = Some(PathBuf::from("/tmp/custom-project.sqlite"));
        assert_eq!(
            app.current_project_slug_for_tmux(),
            Some("custom-project".to_string())
        );

        Ok(())
    }

    #[test]
    fn attach_task_with_runtime_handles_repo_unavailable_and_missing_worktree() -> Result<()> {
        let (app, _repo_dir, task_id, _category_ids) = test_app_with_middle_task()?;
        let repo = app.repos[0].clone();
        let task = app.db.get_task(task_id)?;

        let mut repo_missing_runtime = RecordingRecoveryRuntime::with_session_exists(false);
        repo_missing_runtime.repo_exists = false;
        let result = attach_task_with_runtime(
            &app.db,
            None,
            &task,
            &repo,
            &[],
            &Theme::default(),
            &repo_missing_runtime,
        )?;
        assert_eq!(result, AttachTaskResult::RepoUnavailable);

        app.db
            .update_task_tmux(task_id, None, Some("/tmp/missing-worktree".to_string()))?;
        let task = app.db.get_task(task_id)?;
        let mut missing_worktree_runtime = RecordingRecoveryRuntime::with_session_exists(false);
        missing_worktree_runtime.worktree_exists = false;
        let result = attach_task_with_runtime(
            &app.db,
            None,
            &task,
            &repo,
            &[],
            &Theme::default(),
            &missing_worktree_runtime,
        )?;
        assert_eq!(result, AttachTaskResult::WorktreeNotFound);

        Ok(())
    }

    #[test]
    fn attach_task_with_runtime_creates_new_session_when_needed() -> Result<()> {
        let (app, _repo_dir, task_id, _category_ids) = test_app_with_middle_task()?;
        let repo = app.repos[0].clone();
        let worktree = TempDir::new()?;

        app.db.update_task_tmux(
            task_id,
            Some("ok-session".to_string()),
            Some(worktree.path().display().to_string()),
        )?;
        let task = app.db.get_task(task_id)?;

        let runtime = RecordingRecoveryRuntime::with_session_exists(false);
        let result = attach_task_with_runtime(
            &app.db,
            None,
            &task,
            &repo,
            &[],
            &Theme::default(),
            &runtime,
        )?;

        assert_eq!(result, AttachTaskResult::Attached);
        assert_eq!(runtime.create_session_calls.get(), 1);
        assert_eq!(runtime.switch_client_calls.get(), 1);
        Ok(())
    }

    #[test]
    fn reconcile_startup_tasks_recovers_running_statuses() -> Result<()> {
        let (mut app, _repo_dir, task_id, _category_ids) = test_app_with_middle_task()?;
        app.db
            .update_task_tmux(task_id, Some("ok-recovery-session".to_string()), None)?;
        app.db
            .update_task_status(task_id, Status::Running.as_str())?;
        app.refresh_data()?;

        let missing_session = RecordingRecoveryRuntime::with_session_exists(false);
        reconcile_startup_tasks(&app.db, &app.tasks, &app.repos, &missing_session)?;
        assert_eq!(app.db.get_task(task_id)?.tmux_status, Status::Idle.as_str());

        app.db
            .update_task_status(task_id, Status::Running.as_str())?;
        app.refresh_data()?;
        let existing_session = RecordingRecoveryRuntime::with_session_exists(true);
        reconcile_startup_tasks(&app.db, &app.tasks, &app.repos, &existing_session)?;
        assert_eq!(
            app.db.get_task(task_id)?.tmux_status,
            Status::Running.as_str()
        );

        app.db
            .update_task_status(task_id, Status::Running.as_str())?;
        app.refresh_data()?;
        let mut repo_missing = RecordingRecoveryRuntime::with_session_exists(true);
        repo_missing.repo_exists = false;
        reconcile_startup_tasks(&app.db, &app.tasks, &app.repos, &repo_missing)?;
        assert_eq!(app.db.get_task(task_id)?.tmux_status, Status::Idle.as_str());

        Ok(())
    }

    #[test]
    fn update_messages_adjust_settings_and_dialog_focus_state() -> Result<()> {
        let (mut app, _repo_dir, _task_id, _category_ids) = test_app_with_middle_task()?;

        app.update(Message::OpenSettings)?;
        app.update(Message::SettingsSelectSection(SettingsSection::Repos))?;
        app.update(Message::SettingsSelectRepo(999))?;
        app.update(Message::SettingsSelectGeneralField(999))?;
        app.update(Message::SettingsSelectCategoryColor(999))?;

        let settings_state = app
            .settings_view_state
            .as_ref()
            .expect("settings state should exist");
        assert_eq!(
            settings_state.active_section,
            SettingsSection::CategoryColors
        );
        assert_eq!(settings_state.general_selected_field, 3);
        assert_eq!(
            settings_state.category_color_selected,
            app.categories.len().saturating_sub(1)
        );

        app.update(Message::OpenNewTaskDialog)?;
        app.update(Message::ToggleNewTaskExistingDirectory)?;
        app.update(Message::SetNewTaskUseExistingDirectory(false))?;
        match &app.active_dialog {
            ActiveDialog::NewTask(state) => {
                assert!(!state.use_existing_directory);
                assert_eq!(state.focused_field, NewTaskField::UseExistingDirectory);
            }
            other => panic!("expected NewTask dialog, got {other:?}"),
        }

        app.update(Message::OpenDeleteTaskDialog)?;
        if let ActiveDialog::DeleteTask(state) = &mut app.active_dialog {
            state.confirm_destructive = true;
        }
        app.update(Message::FocusDeleteTaskField(DeleteTaskField::KillTmux))?;
        match &app.active_dialog {
            ActiveDialog::DeleteTask(state) => {
                assert_eq!(state.focused_field, DeleteTaskField::KillTmux);
                assert!(!state.confirm_destructive);
            }
            other => panic!("expected DeleteTask dialog, got {other:?}"),
        }

        app.update(Message::OpenCommandPalette)?;
        app.update(Message::SelectCommandPaletteItem(0))?;
        assert_eq!(app.current_view, View::ProjectList);

        Ok(())
    }

    #[test]
    fn key_shortcuts_cover_help_side_panel_and_expanded_log_paths() -> Result<()> {
        let (mut app, _repo_dir, _task_id, _category_ids) = test_app_with_middle_task()?;

        let toggle_help_key = find_toggle_help_key(&app);
        app.active_dialog = ActiveDialog::Help;
        app.handle_key(toggle_help_key)?;
        assert_eq!(app.active_dialog, ActiveDialog::None);

        app.view_mode = ViewMode::SidePanel;
        app.current_view = View::Board;
        app.detail_focus = DetailFocus::Log;
        app.current_log_buffer = Some(
            "> [SAY] ASSISTANT 10:00:00\n  first\n\n> [SAY] ASSISTANT 10:01:00\n  second"
                .to_string(),
        );
        app.handle_key(key_char('f'))?;
        assert!(app.log_expanded);

        app.log_expanded_scroll_offset = 1;
        app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::empty()))?;
        assert!(!app.log_expanded);
        assert_eq!(app.log_scroll_offset, 1);

        app.detail_focus = DetailFocus::Log;
        app.handle_key(KeyEvent::new(KeyCode::Char('+'), KeyModifiers::empty()))?;
        assert_eq!(app.log_split_ratio, 60);
        app.handle_key(KeyEvent::new(KeyCode::Char('-'), KeyModifiers::empty()))?;
        assert_eq!(app.log_split_ratio, 65);

        assert!(app.collapsed_categories.is_empty());
        app.handle_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::empty()))?;
        assert!(!app.collapsed_categories.is_empty());

        Ok(())
    }

    #[test]
    fn mouse_scroll_routes_for_archive_and_settings_views() -> Result<()> {
        let (mut app, _repo_dir, _task_id, category_ids) = test_app_with_middle_task()?;

        app.current_view = View::Archive;
        app.archived_tasks = vec![
            test_task(category_ids[0], 0, "archived-1"),
            test_task(category_ids[0], 1, "archived-2"),
        ];
        app.archive_selected_index = 0;
        app.handle_scroll(0, 0, 1)?;
        assert_eq!(app.archive_selected_index, 1);
        app.handle_scroll(0, 0, -1)?;
        assert_eq!(app.archive_selected_index, 0);

        app.current_view = View::Settings;
        app.settings_view_state = Some(SettingsViewState {
            active_section: SettingsSection::General,
            general_selected_field: 0,
            category_color_selected: 0,
            repos_selected_field: 0,
            previous_view: View::Board,
        });
        app.handle_scroll(0, 0, 1)?;
        assert_eq!(
            app.settings_view_state
                .as_ref()
                .map(|state| state.general_selected_field),
            Some(1)
        );
        app.handle_scroll(0, 0, -1)?;
        assert_eq!(
            app.settings_view_state
                .as_ref()
                .map(|state| state.general_selected_field),
            Some(0)
        );

        Ok(())
    }

    #[test]
    fn navigation_helpers_manage_log_entries_and_empty_selection_reset() -> Result<()> {
        let (mut app, _repo_dir, _task_id, _category_ids) = test_app_with_middle_task()?;

        app.current_log_buffer = Some(
            "> [SAY] ASSISTANT 10:00:00\n  one\n\n> [TOOL] ASSISTANT 10:01:00\n  two".to_string(),
        );
        assert_eq!(app.log_entry_count(), 2);

        app.log_scroll_offset = 1;
        app.toggle_selected_log_entry(false);
        assert!(app.log_expanded_entries.contains(&1));
        app.toggle_selected_log_entry(false);
        assert!(!app.log_expanded_entries.contains(&1));

        app.current_log_buffer = Some("alpha\n\n beta".to_string());
        assert_eq!(app.log_entry_count(), 2);

        app.current_log_buffer = Some("present".to_string());
        app.current_change_summary = Some(GitChangeSummary {
            base_ref: "main".to_string(),
            commits_ahead: 1,
            files_changed: 1,
            insertions: 2,
            deletions: 0,
            top_files: vec!["src/app/mod.rs".to_string()],
        });
        app.detail_scroll_offset = 5;
        app.log_scroll_offset = 4;
        app.log_expanded_scroll_offset = 3;
        app.log_expanded_entries.insert(0);

        app.sync_side_panel_selection_at(&[], 99, true);
        assert_eq!(app.side_panel_selected_row, 0);
        assert!(app.current_log_buffer.is_none());
        assert!(app.current_change_summary.is_none());
        assert_eq!(app.detail_scroll_offset, 0);
        assert_eq!(app.log_scroll_offset, 0);
        assert_eq!(app.log_expanded_scroll_offset, 0);
        assert!(app.log_expanded_entries.is_empty());

        Ok(())
    }
}

fn default_db_path() -> Result<PathBuf> {
    let path = projects::get_project_path(projects::DEFAULT_PROJECT);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create data dir {}", parent.display()))?;
    }
    Ok(path)
}

pub fn point_in_rect(x: u16, y: u16, rect: Rect) -> bool {
    x >= rect.x
        && x < rect.x.saturating_add(rect.width)
        && y >= rect.y
        && y < rect.y.saturating_add(rect.height)
}
