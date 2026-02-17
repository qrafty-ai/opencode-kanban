pub mod actions;
pub mod dialogs;
pub mod messages;
pub mod polling;
pub mod runtime;
pub mod state;

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use crossterm::event::{KeyEvent, MouseEvent};
use tracing::warn;
use tuirealm::ratatui::layout::Rect;
use tuirealm::ratatui::widgets::{ListState, ScrollbarState};
use uuid::Uuid;

pub use self::messages::Message;
pub use self::state::{
    ActiveDialog, CategoryInputDialogState, CategoryInputField, CategoryInputMode,
    ConfirmQuitDialogState, ContextMenuItem, ContextMenuState, DeleteCategoryDialogState,
    DeleteCategoryField, DeleteTaskDialogState, DeleteTaskField, ErrorDialogState,
    MoveTaskDialogState, NewProjectDialogState, NewProjectField, NewTaskDialogState, NewTaskField,
    RepoUnavailableDialogState, STATUS_BROKEN, STATUS_REPO_UNAVAILABLE, View, ViewMode,
    WorktreeNotFoundDialogState, WorktreeNotFoundField,
};

use crate::command_palette::{CommandPaletteState, all_commands};
use crate::db::Database;
use crate::git::{derive_worktree_path, git_delete_branch, git_remove_worktree};
use crate::keybindings::{KeyAction, KeyContext, Keybindings};
use crate::opencode::{
    OpenCodeServerManager, Status, ensure_server_ready, opencode_attach_command,
};
use crate::projects::{self, ProjectInfo};
use crate::theme::{Theme, ThemePreset};
use crate::tmux::{tmux_capture_pane, tmux_kill_session};
use crate::types::{Category, Repo, Task};

use self::runtime::{
    CreateTaskRuntime, RealCreateTaskRuntime, RealRecoveryRuntime, RecoveryRuntime,
    next_available_session_name, next_available_session_name_by, worktrees_root_for_repo,
};
use self::state::{AttachTaskResult, CreateTaskOutcome, DesiredTaskState, ObservedTaskState};

pub struct App {
    pub should_quit: bool,
    pub pulse_phase: u8,
    pub theme: Theme,
    pub layout_epoch: u64,
    pub viewport: (u16, u16),
    pub last_mouse_event: Option<MouseEvent>,
    pub db: Database,
    pub tasks: Vec<Task>,
    pub categories: Vec<Category>,
    pub repos: Vec<Repo>,
    pub focused_column: usize,
    pub selected_task_per_column: HashMap<usize, usize>,
    pub scroll_offset_per_column: HashMap<usize, usize>,
    pub column_scroll_states: Vec<ScrollbarState>,
    pub active_dialog: ActiveDialog,
    pub footer_notice: Option<String>,
    pub hit_test_map: Vec<(Rect, Message)>,
    pub hovered_message: Option<Message>,
    pub context_menu: Option<ContextMenuState>,
    pub current_view: View,
    pub current_project_path: Option<PathBuf>,
    pub project_list: Vec<ProjectInfo>,
    pub selected_project_index: usize,
    pub project_list_state: ListState,
    started_at: Instant,
    mouse_seen: bool,
    mouse_hint_shown: bool,
    _server_manager: OpenCodeServerManager,
    poller_stop: Arc<AtomicBool>,
    poller_thread: Option<thread::JoinHandle<()>>,
    pub view_mode: ViewMode,
    pub side_panel_width: u16,
    pub selected_task_index: usize,
    pub current_log_buffer: Option<String>,
    pub keybindings: Keybindings,
}

impl App {
    pub fn active_session_count(&self) -> usize {
        self.tasks
            .iter()
            .filter(|t| t.tmux_status == "running")
            .count()
    }

    pub fn new(project_name: Option<&str>) -> Result<Self> {
        let preset = std::env::var("OPENCODE_KANBAN_THEME")
            .ok()
            .and_then(|value| ThemePreset::from_str(&value).ok())
            .unwrap_or_default();
        Self::new_with_theme(project_name, preset)
    }

    pub fn new_with_theme(project_name: Option<&str>, preset: ThemePreset) -> Result<Self> {
        let db_path = default_db_path()?;
        let db = Database::open(&db_path)?;
        let server_manager = ensure_server_ready();
        let poller_stop = Arc::new(AtomicBool::new(false));

        let mut app = Self {
            should_quit: false,
            pulse_phase: 0,
            theme: Theme::from_preset(preset),
            layout_epoch: 0,
            viewport: (80, 24),
            last_mouse_event: None,
            db,
            tasks: Vec::new(),
            categories: Vec::new(),
            repos: Vec::new(),
            focused_column: 0,
            selected_task_per_column: HashMap::new(),
            scroll_offset_per_column: HashMap::new(),
            column_scroll_states: Vec::new(),
            active_dialog: ActiveDialog::None,
            footer_notice: None,
            hit_test_map: Vec::new(),
            hovered_message: None,
            context_menu: None,
            current_view: View::ProjectList,
            current_project_path: None,
            project_list: Vec::new(),
            selected_project_index: 0,
            project_list_state: ListState::default(),
            started_at: Instant::now(),
            mouse_seen: false,
            mouse_hint_shown: false,
            _server_manager: server_manager,
            poller_stop,
            poller_thread: None,
            view_mode: ViewMode::SidePanel,
            side_panel_width: 40,
            selected_task_index: 0,
            current_log_buffer: None,
            keybindings: Keybindings::load(),
        };

        app.refresh_data()?;
        app.refresh_projects()?;

        if let Some(name) = project_name {
            if let Some(idx) = app.project_list.iter().position(|p| p.name == name) {
                app.selected_project_index = idx;
                if let Some(project) = app.project_list.get(idx) {
                    app.switch_project(project.path.clone())?;
                    app.current_view = View::Board;
                }
            } else {
                anyhow::bail!("project '{}' not found", name);
            }
        }

        app.reconcile_startup_with_runtime(&RealRecoveryRuntime)?;
        app.refresh_data()?;

        app.poller_thread = Some(polling::spawn_status_poller(
            db_path,
            Arc::clone(&app.poller_stop),
        ));
        Ok(app)
    }

    pub fn should_quit(&self) -> bool {
        self.should_quit
    }

    pub fn refresh_data(&mut self) -> Result<()> {
        self.tasks = self.db.list_tasks().context("failed to load tasks")?;
        self.categories = self
            .db
            .list_categories()
            .context("failed to load categories")?;
        self.repos = self.db.list_repos().context("failed to load repos")?;

        if !self.categories.is_empty() {
            self.focused_column = self.focused_column.min(self.categories.len() - 1);
            self.selected_task_per_column
                .entry(self.focused_column)
                .or_insert(0);
            self.scroll_offset_per_column
                .entry(self.focused_column)
                .or_insert(0);

            let num_columns = self.categories.len();
            self.column_scroll_states = (0..num_columns)
                .map(|i| {
                    let task_count = self
                        .tasks
                        .iter()
                        .filter(|t| t.category_id == self.categories[i].id)
                        .count();
                    ScrollbarState::new(task_count.saturating_sub(1))
                })
                .collect();
        }

        Ok(())
    }

    pub fn refresh_projects(&mut self) -> Result<()> {
        self.project_list = projects::list_projects().context("failed to list projects")?;
        if !self.project_list.is_empty() {
            self.selected_project_index =
                self.selected_project_index.min(self.project_list.len() - 1);
            self.project_list_state
                .select(Some(self.selected_project_index));
        } else {
            self.selected_project_index = 0;
            self.project_list_state.select(None);
        }
        Ok(())
    }

    pub fn switch_project(&mut self, path: PathBuf) -> Result<()> {
        self.poller_stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.poller_thread.take() {
            let _ = handle.join();
        }

        let db = Database::open(&path)?;
        self.db = db;
        self.refresh_data()?;

        self.poller_stop.store(false, Ordering::Relaxed);
        self.poller_thread = Some(polling::spawn_status_poller(
            path.clone(),
            Arc::clone(&self.poller_stop),
        ));

        self.current_project_path = Some(path);
        Ok(())
    }

    fn current_project_slug_for_tmux(&self) -> Option<String> {
        let path = self.current_project_path.as_ref()?;
        let stem = path.file_stem()?.to_str()?;
        if stem == projects::DEFAULT_PROJECT {
            None
        } else {
            Some(stem.to_string())
        }
    }

    pub fn update(&mut self, message: Message) -> Result<()> {
        match message {
            Message::Key(key) => self.handle_key(key)?,
            Message::Mouse(mouse) => self.handle_mouse(mouse)?,
            Message::Tick => {
                self.pulse_phase = (self.pulse_phase + 1) % 4;
                self.refresh_data()?;

                if self.view_mode == ViewMode::SidePanel {
                    let Some(task) = self.selected_task() else {
                        self.current_log_buffer = None;
                        self.maybe_show_tmux_mouse_hint();
                        return Ok(());
                    };

                    if task.tmux_status == Status::Running.as_str()
                        && let Some(session_name) = task.tmux_session_name.as_deref()
                    {
                        match tmux_capture_pane(session_name, 50) {
                            Ok(buffer) => self.current_log_buffer = Some(buffer),
                            Err(err) => {
                                warn!(
                                    session = %session_name,
                                    error = %err,
                                    "failed to capture tmux pane"
                                );
                                self.current_log_buffer = None;
                            }
                        }
                    } else {
                        self.current_log_buffer = None;
                    }
                }
            }
            Message::Resize(w, h) => {
                self.viewport = (w, h);
                self.layout_epoch = self.layout_epoch.saturating_add(1);
                self.hit_test_map.clear();
                self.context_menu = None;
                self.hovered_message = None;
            }
            Message::NavigateLeft => {
                if self.focused_column > 0 {
                    self.focused_column -= 1;
                }
            }
            Message::NavigateRight => {
                if self.focused_column + 1 < self.categories.len() {
                    self.focused_column += 1;
                }
            }
            Message::SelectUp => {
                if let Some(selected) = self.selected_task_per_column.get_mut(&self.focused_column)
                {
                    *selected = selected.saturating_sub(1);
                }
            }
            Message::SelectDown => {
                let max_index = self.tasks_in_column(self.focused_column).saturating_sub(1);
                let selected = self
                    .selected_task_per_column
                    .entry(self.focused_column)
                    .or_insert(0);
                *selected = (*selected + 1).min(max_index);
            }
            Message::AttachSelectedTask => self.attach_selected_task()?,
            Message::OpenNewTaskDialog => {
                let default_base = self
                    .repos
                    .first()
                    .and_then(|repo| repo.default_base.clone())
                    .unwrap_or_else(|| "main".to_string());
                self.active_dialog = ActiveDialog::NewTask(NewTaskDialogState {
                    repo_idx: 0,
                    repo_input: String::new(),
                    branch_input: String::new(),
                    base_input: default_base,
                    title_input: String::new(),
                    ensure_base_up_to_date: true,
                    loading_message: None,
                    focused_field: NewTaskField::Repo,
                });
            }
            Message::OpenCommandPalette => {
                let frequencies = self.db.get_command_frequencies().unwrap_or_default();
                self.active_dialog =
                    ActiveDialog::CommandPalette(CommandPaletteState::new(frequencies));
            }
            Message::DismissDialog => {
                self.active_dialog = ActiveDialog::None;
                self.context_menu = None;
                self.hovered_message = None;
            }
            Message::OpenProjectList => {
                self.current_view = View::ProjectList;
                self.active_dialog = ActiveDialog::None;
            }
            Message::FocusColumn(index) => {
                if index < self.categories.len() {
                    self.focused_column = index;
                    self.selected_task_per_column.entry(index).or_insert(0);
                }
            }
            Message::SelectTask(column, index) => {
                if column < self.categories.len() {
                    self.focused_column = column;
                    self.selected_task_per_column.insert(column, index);
                }
            }
            Message::SelectTaskInSidePanel(index) => {
                self.selected_task_index = index;
            }
            Message::OpenAddCategoryDialog => {
                self.active_dialog = ActiveDialog::CategoryInput(CategoryInputDialogState {
                    mode: CategoryInputMode::Add,
                    category_id: None,
                    name_input: String::new(),
                    focused_field: CategoryInputField::Name,
                });
            }
            Message::OpenRenameCategoryDialog => {
                if let Some(category) = self.categories.get(self.focused_column) {
                    self.active_dialog = ActiveDialog::CategoryInput(CategoryInputDialogState {
                        mode: CategoryInputMode::Rename,
                        category_id: Some(category.id),
                        name_input: category.name.clone(),
                        focused_field: CategoryInputField::Name,
                    });
                }
            }
            Message::OpenDeleteCategoryDialog => self.open_delete_category_dialog()?,
            Message::OpenDeleteTaskDialog => self.open_delete_task_dialog()?,
            Message::SubmitCategoryInput => self.confirm_category_input()?,
            Message::ConfirmDeleteCategory => self.confirm_delete_category()?,
            Message::MoveTaskLeft => self.move_task_left()?,
            Message::MoveTaskRight => self.move_task_right()?,
            Message::MoveTaskUp => self.move_task_up()?,
            Message::MoveTaskDown => self.move_task_down()?,
            Message::WorktreeNotFoundRecreate => self.recreate_from_repo_root()?,
            Message::WorktreeNotFoundMarkBroken => self.mark_worktree_missing_as_broken()?,
            Message::RepoUnavailableDismiss => self.active_dialog = ActiveDialog::None,
            Message::CreateTask => self.confirm_new_task()?,
            Message::ConfirmQuit => self.should_quit = true,
            Message::CancelQuit => self.active_dialog = ActiveDialog::None,
            Message::ExecuteCommand(command_id) => {
                self.active_dialog = ActiveDialog::None;

                match command_id.as_str() {
                    "help" => self.active_dialog = ActiveDialog::Help,
                    "quit" => self.should_quit = true,
                    _ => {
                        if let Some(message) = all_commands()
                            .into_iter()
                            .find(|command| command.id == command_id)
                            .and_then(|command| command.message)
                        {
                            self.update(message)?;
                        }
                    }
                }

                let _ = self.db.increment_command_usage(&command_id);
            }
            Message::CycleCategoryColor(col_idx) => {
                let color_cycle = [
                    None,
                    Some("cyan".to_string()),
                    Some("magenta".to_string()),
                    Some("blue".to_string()),
                    Some("green".to_string()),
                    Some("yellow".to_string()),
                    Some("red".to_string()),
                ];
                if let Some(category) = self.categories.get(col_idx) {
                    let current_color = category.color.as_ref();
                    let next_idx = color_cycle
                        .iter()
                        .position(|c| c.as_ref() == current_color)
                        .map(|i| (i + 1) % color_cycle.len())
                        .unwrap_or(0);
                    let next_color = color_cycle[next_idx].clone();
                    self.db
                        .update_category_color(category.id, next_color)
                        .context("failed to update category color")?;
                    self.refresh_data()?;
                }
            }
            Message::DeleteTaskToggleKillTmux
            | Message::DeleteTaskToggleRemoveWorktree
            | Message::DeleteTaskToggleDeleteBranch => {}
            Message::ConfirmDeleteTask => self.confirm_delete_task()?,
            Message::SwitchToProjectList => {
                self.current_view = View::ProjectList;
            }
            Message::SwitchToBoard(path) => {
                self.switch_project(path)?;
                self.current_view = View::Board;
            }
            Message::ProjectListSelectUp => {
                if self.selected_project_index > 0 {
                    self.selected_project_index -= 1;
                    self.project_list_state
                        .select(Some(self.selected_project_index));
                }
            }
            Message::ProjectListSelectDown => {
                if self.selected_project_index + 1 < self.project_list.len() {
                    self.selected_project_index += 1;
                    self.project_list_state
                        .select(Some(self.selected_project_index));
                }
            }
            Message::ProjectListConfirm => {
                if let Some(project) = self.project_list.get(self.selected_project_index) {
                    self.switch_project(project.path.clone())?;
                    self.current_view = View::Board;
                }
            }
            Message::OpenNewProjectDialog => {
                self.active_dialog = ActiveDialog::NewProject(NewProjectDialogState {
                    name_input: String::new(),
                    focused_field: NewProjectField::Name,
                    error_message: None,
                });
            }
            Message::CreateProject => {
                if let ActiveDialog::NewProject(state) = &self.active_dialog {
                    let name = state.name_input.trim();
                    if name.is_empty() {
                    } else {
                        match projects::create_project(name) {
                            Ok(path) => {
                                self.active_dialog = ActiveDialog::None;
                                self.refresh_projects()?;
                                if let Some(idx) =
                                    self.project_list.iter().position(|p| p.path == path)
                                {
                                    self.selected_project_index = idx;
                                    self.project_list_state.select(Some(idx));
                                }
                                self.switch_project(path)?;
                                self.current_view = View::Board;
                            }
                            Err(e) => {
                                self.active_dialog = ActiveDialog::Error(ErrorDialogState {
                                    title: "Failed to create project".to_string(),
                                    detail: e.to_string(),
                                });
                            }
                        }
                    }
                }
            }
            Message::FocusNewTaskField(field) => {
                if let ActiveDialog::NewTask(state) = &mut self.active_dialog {
                    state.focused_field = field;
                }
            }
            Message::ToggleNewTaskCheckbox => {
                if let ActiveDialog::NewTask(state) = &mut self.active_dialog {
                    state.focused_field = NewTaskField::EnsureBaseUpToDate;
                    state.ensure_base_up_to_date = !state.ensure_base_up_to_date;
                }
            }
            Message::FocusCategoryInputField(field) => {
                if let ActiveDialog::CategoryInput(state) = &mut self.active_dialog {
                    state.focused_field = field;
                }
            }
            Message::FocusNewProjectField(field) => {
                if let ActiveDialog::NewProject(state) = &mut self.active_dialog {
                    state.focused_field = field;
                }
            }
            Message::FocusDeleteTaskField(field) => {
                if let ActiveDialog::DeleteTask(state) = &mut self.active_dialog {
                    state.focused_field = field;
                }
            }
            Message::ToggleDeleteTaskCheckbox(field) => {
                if let ActiveDialog::DeleteTask(state) = &mut self.active_dialog {
                    state.focused_field = field;
                    match field {
                        DeleteTaskField::KillTmux => state.kill_tmux = !state.kill_tmux,
                        DeleteTaskField::RemoveWorktree => {
                            state.remove_worktree = !state.remove_worktree
                        }
                        DeleteTaskField::DeleteBranch => state.delete_branch = !state.delete_branch,
                        _ => {}
                    }
                }
            }
            Message::FocusDialogButton(_button_id) => {}
            Message::SelectProject(idx) => {
                if idx < self.project_list.len() {
                    self.selected_project_index = idx;
                    self.project_list_state.select(Some(idx));
                    if let Some(project) = self.project_list.get(idx) {
                        let _ = self.switch_project(project.path.clone());
                        self.current_view = View::Board;
                    }
                }
            }
            #[allow(clippy::collapsible_if)]
            Message::SelectCommandPaletteItem(idx) => {
                if let ActiveDialog::CommandPalette(ref mut state) = self.active_dialog {
                    if idx < state.filtered.len() {
                        state.selected_index = idx;
                        if let Some(cmd_id) = state.selected_command_id() {
                            self.update(Message::ExecuteCommand(cmd_id))?;
                        }
                    }
                }
            }
        }

        self.maybe_show_tmux_mouse_hint();

        Ok(())
    }

    fn handle_key(&mut self, key: KeyEvent) -> Result<()> {
        if self.active_dialog != ActiveDialog::None {
            if let ActiveDialog::Help = self.active_dialog
                && self.keybindings.action_for_key(KeyContext::Global, key)
                    == Some(KeyAction::ToggleHelp)
            {
                self.active_dialog = ActiveDialog::None;
                return Ok(());
            }
            return self.handle_dialog_key(key);
        }

        if let Some(action) = self.keybindings.action_for_key(KeyContext::Global, key) {
            match action {
                KeyAction::ToggleHelp => self.active_dialog = ActiveDialog::Help,
                KeyAction::OpenPalette => {
                    self.update(Message::OpenCommandPalette)?;
                }
                KeyAction::Quit => self.should_quit = true,
                KeyAction::ToggleView => {
                    self.current_log_buffer = None;

                    match self.view_mode {
                        ViewMode::Kanban => {
                            self.view_mode = ViewMode::SidePanel;

                            let entries = self.linear_task_entries();
                            if entries.is_empty() {
                                self.selected_task_index = 0;
                            } else {
                                let current_id = self
                                    .selected_task_in_column(self.focused_column)
                                    .map(|task| task.id);
                                let index = current_id
                                    .and_then(|id| {
                                        entries.iter().position(|(_, _, task)| task.id == id)
                                    })
                                    .unwrap_or(0);
                                self.apply_linear_task_selection(&entries, index);
                            }
                        }
                        ViewMode::SidePanel => {
                            self.view_mode = ViewMode::Kanban;
                        }
                    }
                }
                KeyAction::ShrinkPanel => {
                    self.side_panel_width = self.side_panel_width.saturating_sub(5).max(20);
                }
                KeyAction::ExpandPanel => {
                    self.side_panel_width = self.side_panel_width.saturating_add(5).min(80);
                }
                _ => {}
            }
            return Ok(());
        }

        if self.current_view == View::ProjectList {
            if let Some(action) = self
                .keybindings
                .action_for_key(KeyContext::ProjectList, key)
            {
                match action {
                    KeyAction::ProjectUp => self.update(Message::ProjectListSelectUp)?,
                    KeyAction::ProjectDown => self.update(Message::ProjectListSelectDown)?,
                    KeyAction::ProjectConfirm => self.update(Message::ProjectListConfirm)?,
                    KeyAction::NewProject => self.update(Message::OpenNewProjectDialog)?,
                    _ => {}
                }
            }
            return Ok(());
        }

        if let Some(action) = self.keybindings.action_for_key(KeyContext::Board, key) {
            match action {
                KeyAction::NavigateLeft => {
                    self.update(Message::NavigateLeft)?;
                }
                KeyAction::NavigateRight => {
                    self.update(Message::NavigateRight)?;
                }
                KeyAction::SelectDown => {
                    if self.view_mode == ViewMode::SidePanel {
                        let entries = self.linear_task_entries();
                        if entries.is_empty() {
                            self.selected_task_index = 0;
                            self.current_log_buffer = None;
                        } else {
                            let current = self.selected_task_index.min(entries.len() - 1);
                            let next = (current + 1) % entries.len();
                            self.apply_linear_task_selection(&entries, next);
                        }
                    } else {
                        self.update(Message::SelectDown)?;
                    }
                }
                KeyAction::SelectUp => {
                    if self.view_mode == ViewMode::SidePanel {
                        let entries = self.linear_task_entries();
                        if entries.is_empty() {
                            self.selected_task_index = 0;
                            self.current_log_buffer = None;
                        } else {
                            let current = self.selected_task_index.min(entries.len() - 1);
                            let prev = if current == 0 {
                                entries.len() - 1
                            } else {
                                current - 1
                            };
                            self.apply_linear_task_selection(&entries, prev);
                        }
                    } else {
                        self.update(Message::SelectUp)?;
                    }
                }
                KeyAction::NewTask => {
                    self.update(Message::OpenNewTaskDialog)?;
                }
                KeyAction::AddCategory => {
                    self.update(Message::OpenAddCategoryDialog)?;
                }
                KeyAction::CycleCategoryColor => {
                    self.update(Message::CycleCategoryColor(self.focused_column))?;
                }
                KeyAction::RenameCategory => {
                    self.update(Message::OpenRenameCategoryDialog)?;
                }
                KeyAction::DeleteCategory => {
                    self.update(Message::OpenDeleteCategoryDialog)?;
                }
                KeyAction::DeleteTask => {
                    self.update(Message::OpenDeleteTaskDialog)?;
                }
                KeyAction::MoveTaskLeft => {
                    self.update(Message::MoveTaskLeft)?;
                }
                KeyAction::MoveTaskRight => {
                    self.update(Message::MoveTaskRight)?;
                }
                KeyAction::MoveTaskDown => {
                    self.update(Message::MoveTaskDown)?;
                }
                KeyAction::MoveTaskUp => {
                    self.update(Message::MoveTaskUp)?;
                }
                KeyAction::AttachTask => {
                    self.update(Message::AttachSelectedTask)?;
                }
                KeyAction::Dismiss => {
                    self.update(Message::DismissDialog)?;
                }
                _ => {}
            }
        }

        Ok(())
    }

    fn handle_mouse(&mut self, _mouse: MouseEvent) -> Result<()> {
        Ok(())
    }

    fn handle_dialog_key(&mut self, key: KeyEvent) -> Result<()> {
        let follow_up = dialogs::handle_dialog_key(
            &mut self.active_dialog,
            key,
            &self.db,
            &mut self.repos,
            &mut self.categories,
            &mut self.focused_column,
        )?;

        if let Some(message) = follow_up {
            self.update(message)?;
        }

        Ok(())
    }

    fn tasks_in_column(&self, column_index: usize) -> usize {
        let Some(category) = self.categories.get(column_index) else {
            return 0;
        };
        self.tasks
            .iter()
            .filter(|task| task.category_id == category.id)
            .count()
    }

    fn max_scroll_offset_for_column(&self, column_index: usize) -> usize {
        self.tasks_in_column(column_index).saturating_sub(1)
    }

    pub fn clamped_scroll_offset_for_column(&self, column_index: usize) -> usize {
        self.scroll_offset_per_column
            .get(&column_index)
            .copied()
            .unwrap_or(0)
            .min(self.max_scroll_offset_for_column(column_index))
    }

    pub fn selected_task(&self) -> Option<Task> {
        match self.view_mode {
            ViewMode::Kanban => self.selected_task_in_column(self.focused_column),
            ViewMode::SidePanel => self.selected_task_in_linear_list(),
        }
    }

    fn selected_task_in_column(&self, column_index: usize) -> Option<Task> {
        let category = self.categories.get(column_index)?;
        let mut tasks: Vec<Task> = self
            .tasks
            .iter()
            .filter(|task| task.category_id == category.id)
            .cloned()
            .collect();
        tasks.sort_by_key(|task| task.position);

        let selected = self
            .selected_task_per_column
            .get(&column_index)
            .copied()
            .unwrap_or(0);
        tasks.get(selected).cloned()
    }

    fn selected_task_in_linear_list(&self) -> Option<Task> {
        let entries = self.linear_task_entries();
        if entries.is_empty() {
            return None;
        }
        let index = self.selected_task_index.min(entries.len() - 1);
        entries.into_iter().nth(index).map(|(_, _, task)| task)
    }

    fn linear_task_entries(&self) -> Vec<(usize, usize, Task)> {
        let mut category_order: Vec<(usize, &Category)> =
            self.categories.iter().enumerate().collect();
        category_order.sort_by_key(|(_, category)| category.position);

        let mut entries: Vec<(usize, usize, Task)> = Vec::new();
        for (column_index, category) in category_order {
            let mut tasks: Vec<Task> = self
                .tasks
                .iter()
                .filter(|task| task.category_id == category.id)
                .cloned()
                .collect();
            tasks.sort_by_key(|task| task.position);

            for (index_in_column, task) in tasks.into_iter().enumerate() {
                entries.push((column_index, index_in_column, task));
            }
        }

        entries
    }

    fn apply_linear_task_selection(&mut self, entries: &[(usize, usize, Task)], index: usize) {
        if entries.is_empty() {
            self.selected_task_index = 0;
            self.current_log_buffer = None;
            return;
        }

        let index = index.min(entries.len() - 1);
        self.selected_task_index = index;

        let (column_index, index_in_column, _) = &entries[index];
        self.focused_column = (*column_index).min(self.categories.len().saturating_sub(1));
        self.selected_task_per_column
            .insert(*column_index, *index_in_column);
        self.current_log_buffer = None;
    }

    fn repo_for_task(&self, task: &Task) -> Option<Repo> {
        self.repos
            .iter()
            .find(|repo| repo.id == task.repo_id)
            .cloned()
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
        let positions: Vec<(Uuid, i64)> = tasks
            .iter()
            .enumerate()
            .map(|(idx, task)| (task.id, idx as i64))
            .collect();
        self.db.reorder_task_positions(&positions)?;
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
        let positions: Vec<(Uuid, i64)> = tasks
            .iter()
            .enumerate()
            .map(|(idx, task)| (task.id, idx as i64))
            .collect();
        self.db.reorder_task_positions(&positions)?;
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
            focused_field: DeleteCategoryField::Cancel,
        });
        Ok(())
    }

    fn open_delete_task_dialog(&mut self) -> Result<()> {
        let Some(task) = self.selected_task() else {
            return Ok(());
        };

        self.active_dialog = ActiveDialog::DeleteTask(DeleteTaskDialogState {
            task_id: task.id,
            task_title: task.title.clone(),
            task_branch: task.branch.clone(),
            kill_tmux: true,
            remove_worktree: true,
            delete_branch: false,
            focused_field: DeleteTaskField::Cancel,
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
        let ActiveDialog::DeleteTask(state) = self.active_dialog.clone() else {
            return Ok(());
        };

        let task = self.tasks.iter().find(|t| t.id == state.task_id);
        let Some(task) = task else {
            self.active_dialog = ActiveDialog::None;
            return Ok(());
        };

        let repo = self.repo_for_task(task);

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
        Ok(())
    }

    fn reconcile_startup_with_runtime(&mut self, runtime: &impl RecoveryRuntime) -> Result<()> {
        reconcile_startup_tasks(&self.db, &self.tasks, &self.repos, runtime)
    }

    fn attach_selected_task(&mut self) -> Result<()> {
        let Some(task) = self.selected_task() else {
            return Ok(());
        };
        let Some(repo) = self.repo_for_task(&task) else {
            return Ok(());
        };

        let project_slug = self.current_project_slug_for_tmux();
        let result = attach_task_with_runtime(
            &self.db,
            project_slug.as_deref(),
            &task,
            &repo,
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

        self.db.update_task_status(task_id, STATUS_BROKEN)?;
        self.active_dialog = ActiveDialog::None;
        self.refresh_data()
    }

    fn confirm_new_task(&mut self) -> Result<()> {
        let ActiveDialog::NewTask(mut dialog_state) = self.active_dialog.clone() else {
            return Ok(());
        };

        dialog_state.loading_message = Some("Fetching git refs and creating task...".to_string());
        self.active_dialog = ActiveDialog::NewTask(dialog_state.clone());

        let todo_category = self
            .categories
            .iter()
            .find(|category| category.name == "TODO")
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
                let detail = format!("{err:#}");
                let title = if detail.contains("worktree creation failed") {
                    "Worktree creation failed".to_string()
                } else if detail.contains("tmux session creation failed") {
                    "Tmux session failed".to_string()
                } else {
                    "Task creation failed".to_string()
                };
                self.active_dialog = ActiveDialog::Error(ErrorDialogState { title, detail });
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

impl Drop for App {
    fn drop(&mut self) {
        self.poller_stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.poller_thread.take() {
            let deadline = Instant::now() + Duration::from_millis(200);
            while !handle.is_finished() && Instant::now() < deadline {
                thread::sleep(Duration::from_millis(10));
            }

            if handle.is_finished() {
                let _ = handle.join();
            }
        }
    }
}

fn desired_state_for_task(task: &Task, repo_available: bool) -> DesiredTaskState {
    DesiredTaskState {
        expected_session_name: task.tmux_session_name.clone(),
        repo_available,
    }
}

fn observed_state_for_task(
    desired: &DesiredTaskState,
    runtime: &impl RecoveryRuntime,
) -> ObservedTaskState {
    if !desired.repo_available {
        return ObservedTaskState {
            repo_available: false,
            session_exists: false,
            session_status: None,
        };
    }

    let Some(session_name) = desired.expected_session_name.as_deref() else {
        return ObservedTaskState {
            repo_available: true,
            session_exists: false,
            session_status: None,
        };
    };

    if !runtime.session_exists(session_name) {
        return ObservedTaskState {
            repo_available: true,
            session_exists: false,
            session_status: None,
        };
    }

    ObservedTaskState {
        repo_available: true,
        session_exists: true,
        session_status: None,
    }
}

fn reconcile_desired_vs_observed(
    desired: &DesiredTaskState,
    observed: &ObservedTaskState,
    current_status: &str,
) -> String {
    if !desired.repo_available || !observed.repo_available {
        return STATUS_REPO_UNAVAILABLE.to_string();
    }

    if desired.expected_session_name.is_none() {
        if current_status == STATUS_REPO_UNAVAILABLE
            || current_status == Status::Dead.as_str()
            || current_status == STATUS_BROKEN
        {
            return Status::Idle.as_str().to_string();
        }
        return current_status.to_string();
    }

    if !observed.session_exists {
        return Status::Dead.as_str().to_string();
    }

    observed
        .session_status
        .as_ref()
        .map(|status| status.state.as_str().to_string())
        .unwrap_or_else(|| current_status.to_string())
}

fn reconcile_startup_tasks(
    db: &Database,
    tasks: &[Task],
    repos: &[Repo],
    runtime: &impl RecoveryRuntime,
) -> Result<()> {
    let repos_by_id: HashMap<Uuid, &Repo> = repos.iter().map(|repo| (repo.id, repo)).collect();

    for task in tasks {
        let repo_available = repos_by_id
            .get(&task.repo_id)
            .map(|repo| runtime.repo_exists(Path::new(&repo.path)))
            .unwrap_or(false);

        let desired = desired_state_for_task(task, repo_available);
        let observed = observed_state_for_task(&desired, runtime);
        let reconciled_status =
            reconcile_desired_vs_observed(&desired, &observed, &task.tmux_status);

        if reconciled_status != task.tmux_status {
            tracing::debug!(
                task_id = %task.id,
                previous = %task.tmux_status,
                reconciled = %reconciled_status,
                "startup recovery reconciliation updated task status"
            );
            db.update_task_status(task.id, &reconciled_status)?;
        }
    }

    Ok(())
}

fn attach_task_with_runtime(
    db: &Database,
    project_slug: Option<&str>,
    task: &Task,
    repo: &Repo,
    runtime: &impl RecoveryRuntime,
) -> Result<AttachTaskResult> {
    if !runtime.repo_exists(Path::new(&repo.path)) {
        db.update_task_status(task.id, STATUS_REPO_UNAVAILABLE)?;
        return Ok(AttachTaskResult::RepoUnavailable);
    }

    if let Some(session_name) = task.tmux_session_name.as_deref()
        && runtime.session_exists(session_name)
    {
        runtime.switch_client(session_name)?;
        return Ok(AttachTaskResult::Attached);
    }

    let Some(worktree_path_str) = task.worktree_path.as_deref() else {
        return Ok(AttachTaskResult::WorktreeNotFound);
    };
    let worktree_path = Path::new(worktree_path_str);
    if !runtime.worktree_exists(worktree_path) {
        return Ok(AttachTaskResult::WorktreeNotFound);
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

    runtime.switch_client(&session_name)?;
    Ok(AttachTaskResult::Attached)
}

fn create_task_pipeline_with_runtime(
    db: &Database,
    repos: &mut Vec<Repo>,
    todo_category_id: Uuid,
    state: &NewTaskDialogState,
    project_slug: Option<&str>,
    runtime: &impl CreateTaskRuntime,
) -> Result<CreateTaskOutcome> {
    let mut warning = None;
    let repo = resolve_repo_for_creation(db, repos, state, runtime)?;
    let repo_path = PathBuf::from(&repo.path);

    let branch = state.branch_input.trim();
    if branch.is_empty() {
        anyhow::bail!("branch cannot be empty");
    }
    runtime
        .git_validate_branch(&repo_path, branch)
        .context("branch validation failed")?;

    let base_ref = if state.base_input.trim().is_empty() {
        runtime.git_detect_default_branch(&repo_path)
    } else {
        state.base_input.trim().to_string()
    };

    if let Err(err) = runtime.git_fetch(&repo_path) {
        let message = format!("fetch from origin failed, continuing offline: {err:#}");
        tracing::warn!("{message}");
        warning = Some(message);
    }

    if state.ensure_base_up_to_date {
        runtime
            .git_check_branch_up_to_date(&repo_path, &base_ref)
            .context("base branch check failed")?;
    }

    let worktrees_root = worktrees_root_for_repo(&repo_path);
    fs::create_dir_all(&worktrees_root).with_context(|| {
        format!(
            "failed to create worktree root {}",
            worktrees_root.display()
        )
    })?;
    let worktree_path = derive_worktree_path(&worktrees_root, &repo_path, branch);

    runtime
        .git_create_worktree(&repo_path, &worktree_path, branch, &base_ref)
        .context("worktree creation failed")?;

    let mut created_session_name: Option<String> = None;
    let mut created_task_id: Option<Uuid> = None;

    let mut operation = || -> Result<()> {
        let session_name =
            next_available_session_name_by(None, project_slug, &repo.name, branch, |name| {
                runtime.tmux_session_exists(name)
            });

        let command = opencode_attach_command(None, Some(worktree_path.to_string_lossy().as_ref()));

        runtime
            .tmux_create_session(&session_name, &worktree_path, Some(&command))
            .context("tmux session creation failed")?;
        created_session_name = Some(session_name.clone());

        let task = db
            .add_task(repo.id, branch, state.title_input.trim(), todo_category_id)
            .context("failed to save task")?;
        created_task_id = Some(task.id);

        db.update_task_tmux(
            task.id,
            Some(session_name.clone()),
            Some(worktree_path.display().to_string()),
        )
        .context("failed to save task runtime metadata")?;
        db.update_task_status(task.id, Status::Idle.as_str())
            .context("failed to save task runtime status")?;

        Ok(())
    };

    if let Err(err) = operation() {
        if let Some(task_id) = created_task_id {
            let _ = db.delete_task(task_id);
        }
        if let Some(session_name) = created_session_name {
            let _ = runtime.tmux_kill_session(&session_name);
        }
        let _ = runtime.git_remove_worktree(&repo_path, &worktree_path);
        return Err(err);
    }

    Ok(CreateTaskOutcome { warning })
}

fn resolve_repo_for_creation(
    db: &Database,
    repos: &mut Vec<Repo>,
    state: &NewTaskDialogState,
    runtime: &impl CreateTaskRuntime,
) -> Result<Repo> {
    let repo_path_input = state.repo_input.trim();
    if !repo_path_input.is_empty() {
        let path = PathBuf::from(repo_path_input);
        if !path.exists() {
            anyhow::bail!("repo path does not exist: {}", path.display());
        }
        if !runtime.git_is_valid_repo(&path) {
            anyhow::bail!("not a git repository: {}", path.display());
        }

        let canonical = fs::canonicalize(&path)
            .with_context(|| format!("failed to canonicalize repo path {}", path.display()))?;
        if let Some(existing) = repos
            .iter()
            .find(|repo| Path::new(&repo.path) == canonical)
            .cloned()
        {
            return Ok(existing);
        }

        let repo = db
            .add_repo(&canonical)
            .with_context(|| format!("failed to save repo {}", canonical.display()))?;
        repos.push(repo.clone());
        return Ok(repo);
    }

    repos
        .get(state.repo_idx)
        .cloned()
        .context("select a repo or enter a repository path")
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
