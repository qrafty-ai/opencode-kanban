use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use crossterm::event::{KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use ratatui::widgets::ScrollbarState;
use tracing::{debug, warn};
use uuid::Uuid;

use crate::db::Database;
use crate::git::{
    derive_worktree_path, git_create_worktree, git_detect_default_branch, git_fetch,
    git_is_valid_repo, git_remove_worktree,
};
use crate::opencode::{Status, opencode_detect_status, opencode_is_running_in_session};
use crate::tmux::{
    sanitize_session_name, tmux_capture_pane, tmux_create_session, tmux_kill_session,
    tmux_send_keys, tmux_session_exists, tmux_switch_client,
};
use crate::types::{Category, Repo, Task};

pub const STATUS_REPO_UNAVAILABLE: &str = "repo_unavailable";
pub const STATUS_BROKEN: &str = "broken";

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum NewTaskField {
    Repo,
    Branch,
    Base,
    Title,
    Create,
    Cancel,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct NewTaskDialogState {
    pub repo_idx: usize,
    pub repo_input: String,
    pub branch_input: String,
    pub base_input: String,
    pub title_input: String,
    pub loading_message: Option<String>,
    pub focused_field: NewTaskField,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ErrorDialogState {
    pub title: String,
    pub detail: String,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum DeleteTaskField {
    KillTmux,
    RemoveWorktree,
    DeleteBranch,
    Delete,
    Cancel,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct DeleteTaskDialogState {
    pub task_title: String,
    pub task_branch: String,
    pub kill_tmux: bool,
    pub remove_worktree: bool,
    pub delete_branch: bool,
    pub focused_field: DeleteTaskField,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct MoveTaskDialogState {
    pub category_idx: usize,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum CategoryInputMode {
    Add,
    Rename,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum CategoryInputField {
    Name,
    Confirm,
    Cancel,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct CategoryInputDialogState {
    pub mode: CategoryInputMode,
    pub category_id: Option<Uuid>,
    pub name_input: String,
    pub focused_field: CategoryInputField,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum DeleteCategoryField {
    Delete,
    Cancel,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct DeleteCategoryDialogState {
    pub category_id: Uuid,
    pub category_name: String,
    pub task_count: usize,
    pub focused_field: DeleteCategoryField,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum WorktreeNotFoundField {
    Recreate,
    MarkBroken,
    Cancel,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct WorktreeNotFoundDialogState {
    pub task_id: Uuid,
    pub task_title: String,
    pub focused_field: WorktreeNotFoundField,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct RepoUnavailableDialogState {
    pub task_title: String,
    pub repo_path: String,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ConfirmQuitDialogState {
    pub active_session_count: usize,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum ActiveDialog {
    None,
    NewTask(NewTaskDialogState),
    CategoryInput(CategoryInputDialogState),
    DeleteCategory(DeleteCategoryDialogState),
    Error(ErrorDialogState),
    DeleteTask(DeleteTaskDialogState),
    MoveTask(MoveTaskDialogState),
    WorktreeNotFound(WorktreeNotFoundDialogState),
    RepoUnavailable(RepoUnavailableDialogState),
    ConfirmQuit(ConfirmQuitDialogState),
    Help,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Message {
    Key(KeyEvent),
    Mouse(MouseEvent),
    Resize(u16, u16),
    NavigateLeft,
    NavigateRight,
    SelectUp,
    SelectDown,
    AttachSelectedTask,
    OpenNewTaskDialog,
    DismissDialog,
    FocusColumn(usize),
    SelectTask(usize, usize),
    OpenAddCategoryDialog,
    OpenRenameCategoryDialog,
    OpenDeleteCategoryDialog,
    SubmitCategoryInput,
    ConfirmDeleteCategory,
    MoveTaskLeft,
    MoveTaskRight,
    MoveTaskUp,
    MoveTaskDown,
    CreateTask,
    DeleteTaskToggleKillTmux,
    DeleteTaskToggleRemoveWorktree,
    DeleteTaskToggleDeleteBranch,
    ConfirmDeleteTask,
    WorktreeNotFoundRecreate,
    WorktreeNotFoundMarkBroken,
    RepoUnavailableDismiss,
    ConfirmQuit,
    CancelQuit,
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct DesiredTaskState {
    expected_session_name: Option<String>,
    repo_available: bool,
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct ObservedTaskState {
    repo_available: bool,
    session_exists: bool,
    session_status: Option<Status>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum AttachTaskResult {
    Attached,
    WorktreeNotFound,
    RepoUnavailable,
}

trait RecoveryRuntime {
    fn repo_exists(&self, repo_path: &Path) -> bool;
    fn worktree_exists(&self, worktree_path: &Path) -> bool;
    fn session_exists(&self, session_name: &str) -> bool;
    fn detect_status(&self, session_name: &str) -> Status;
    fn create_session(&self, session_name: &str, working_dir: &Path, command: &str) -> Result<()>;
    fn send_command(&self, session_name: &str, command: &str) -> Result<()>;
    fn switch_client(&self, session_name: &str) -> Result<()>;
}

struct RealRecoveryRuntime;

impl RecoveryRuntime for RealRecoveryRuntime {
    fn repo_exists(&self, repo_path: &Path) -> bool {
        repo_path.exists()
    }

    fn worktree_exists(&self, worktree_path: &Path) -> bool {
        worktree_path.exists()
    }

    fn session_exists(&self, session_name: &str) -> bool {
        tmux_session_exists(session_name)
    }

    fn detect_status(&self, session_name: &str) -> Status {
        detect_session_status(session_name)
    }

    fn create_session(&self, session_name: &str, working_dir: &Path, command: &str) -> Result<()> {
        tmux_create_session(session_name, working_dir, Some(command))
    }

    fn send_command(&self, session_name: &str, command: &str) -> Result<()> {
        tmux_send_keys(session_name, command)
    }

    fn switch_client(&self, session_name: &str) -> Result<()> {
        tmux_switch_client(session_name)
    }
}

trait CreateTaskRuntime {
    fn git_is_valid_repo(&self, path: &Path) -> bool;
    fn git_detect_default_branch(&self, repo_path: &Path) -> String;
    fn git_fetch(&self, repo_path: &Path) -> Result<()>;
    fn git_validate_branch(&self, repo_path: &Path, branch_name: &str) -> Result<()>;
    fn git_create_worktree(
        &self,
        repo_path: &Path,
        worktree_path: &Path,
        branch_name: &str,
        base_ref: &str,
    ) -> Result<()>;
    fn git_remove_worktree(&self, repo_path: &Path, worktree_path: &Path) -> Result<()>;
    fn tmux_session_exists(&self, session_name: &str) -> bool;
    fn tmux_create_session(
        &self,
        session_name: &str,
        working_dir: &Path,
        command: Option<&str>,
    ) -> Result<()>;
    fn tmux_kill_session(&self, session_name: &str) -> Result<()>;
}

struct RealCreateTaskRuntime;

impl CreateTaskRuntime for RealCreateTaskRuntime {
    fn git_is_valid_repo(&self, path: &Path) -> bool {
        git_is_valid_repo(path)
    }

    fn git_detect_default_branch(&self, repo_path: &Path) -> String {
        git_detect_default_branch(repo_path)
    }

    fn git_fetch(&self, repo_path: &Path) -> Result<()> {
        git_fetch(repo_path)
    }

    fn git_validate_branch(&self, repo_path: &Path, branch_name: &str) -> Result<()> {
        let output = std::process::Command::new("git")
            .args(["check-ref-format", "--branch", branch_name])
            .current_dir(repo_path)
            .output()
            .with_context(|| {
                format!(
                    "failed to validate branch name `{branch_name}` in {}",
                    repo_path.display()
                )
            })?;

        if output.status.success() {
            Ok(())
        } else {
            anyhow::bail!(
                "invalid branch name `{branch_name}`\nstdout: {}\nstderr: {}",
                String::from_utf8_lossy(&output.stdout).trim(),
                String::from_utf8_lossy(&output.stderr).trim()
            )
        }
    }

    fn git_create_worktree(
        &self,
        repo_path: &Path,
        worktree_path: &Path,
        branch_name: &str,
        base_ref: &str,
    ) -> Result<()> {
        git_create_worktree(repo_path, worktree_path, branch_name, base_ref)
    }

    fn git_remove_worktree(&self, repo_path: &Path, worktree_path: &Path) -> Result<()> {
        git_remove_worktree(repo_path, worktree_path)
    }

    fn tmux_session_exists(&self, session_name: &str) -> bool {
        tmux_session_exists(session_name)
    }

    fn tmux_create_session(
        &self,
        session_name: &str,
        working_dir: &Path,
        command: Option<&str>,
    ) -> Result<()> {
        tmux_create_session(session_name, working_dir, command)
    }

    fn tmux_kill_session(&self, session_name: &str) -> Result<()> {
        tmux_kill_session(session_name)
    }
}

#[derive(Debug, Clone)]
struct CreateTaskOutcome {
    warning: Option<String>,
}

pub struct App {
    should_quit: bool,
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
    started_at: Instant,
    mouse_seen: bool,
    mouse_hint_shown: bool,
    poller_stop: Arc<AtomicBool>,
    poller_thread: Option<thread::JoinHandle<()>>,
}

impl App {
    pub fn new() -> Result<Self> {
        let db_path = default_db_path()?;
        let db = Database::open(&db_path)?;
        let poller_stop = Arc::new(AtomicBool::new(false));

        let mut app = Self {
            should_quit: false,
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
            started_at: Instant::now(),
            mouse_seen: false,
            mouse_hint_shown: false,
            poller_stop,
            poller_thread: None,
        };

        app.refresh_data()?;
        app.reconcile_startup_with_runtime(&RealRecoveryRuntime)?;
        app.refresh_data()?;

        app.poller_thread = Some(spawn_status_poller(db_path, Arc::clone(&app.poller_stop)));
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

            // Initialize scroll states for each column
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

    pub fn update(&mut self, message: Message) -> Result<()> {
        match message {
            Message::Key(key) => self.handle_key(key)?,
            Message::Mouse(mouse) => self.handle_mouse(mouse)?,
            Message::Resize(w, h) => {
                self.viewport = (w, h);
                self.layout_epoch = self.layout_epoch.saturating_add(1);
                self.hit_test_map.clear();
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
                    loading_message: None,
                    focused_field: NewTaskField::Repo,
                });
            }
            Message::DismissDialog => self.active_dialog = ActiveDialog::None,
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
            Message::DeleteTaskToggleKillTmux
            | Message::DeleteTaskToggleRemoveWorktree
            | Message::DeleteTaskToggleDeleteBranch
            | Message::ConfirmDeleteTask => {}
        }

        self.maybe_show_tmux_mouse_hint();

        Ok(())
    }

    fn handle_key(&mut self, key: KeyEvent) -> Result<()> {
        if self.active_dialog != ActiveDialog::None {
            if let ActiveDialog::Help = self.active_dialog
                && key.code == KeyCode::Char('?')
            {
                self.active_dialog = ActiveDialog::None;
                return Ok(());
            }
            return self.handle_dialog_key(key);
        }

        match key.code {
            KeyCode::Char('?') => self.active_dialog = ActiveDialog::Help,
            KeyCode::Char('q') => {
                let active_session_count = self.active_session_count();
                if active_session_count > 0 {
                    self.active_dialog = ActiveDialog::ConfirmQuit(ConfirmQuitDialogState {
                        active_session_count,
                    });
                } else {
                    self.should_quit = true;
                }
            }
            KeyCode::Char('h') | KeyCode::Left => {
                self.update(Message::NavigateLeft)?;
            }
            KeyCode::Char('l') | KeyCode::Right => {
                self.update(Message::NavigateRight)?;
            }
            KeyCode::Char('j') | KeyCode::Down => {
                self.update(Message::SelectDown)?;
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.update(Message::SelectUp)?;
            }
            KeyCode::Char('n') => {
                self.update(Message::OpenNewTaskDialog)?;
            }
            KeyCode::Char('c') => {
                self.update(Message::OpenAddCategoryDialog)?;
            }
            KeyCode::Char('r') => {
                self.update(Message::OpenRenameCategoryDialog)?;
            }
            KeyCode::Char('x') => {
                self.update(Message::OpenDeleteCategoryDialog)?;
            }
            KeyCode::Char('H') => {
                self.update(Message::MoveTaskLeft)?;
            }
            KeyCode::Char('L') => {
                self.update(Message::MoveTaskRight)?;
            }
            KeyCode::Char('J') => {
                self.update(Message::MoveTaskDown)?;
            }
            KeyCode::Char('K') => {
                self.update(Message::MoveTaskUp)?;
            }
            KeyCode::Enter => {
                self.update(Message::AttachSelectedTask)?;
            }
            KeyCode::Esc => {
                self.update(Message::DismissDialog)?;
            }
            _ => {}
        }

        Ok(())
    }

    fn handle_mouse(&mut self, mouse: MouseEvent) -> Result<()> {
        self.last_mouse_event = Some(mouse);
        self.mouse_seen = true;

        match mouse.kind {
            MouseEventKind::ScrollDown => {
                if self.categories.get(self.focused_column).is_some() {
                    let max_offset = self.max_scroll_offset_for_column(self.focused_column);
                    let offset = self
                        .scroll_offset_per_column
                        .entry(self.focused_column)
                        .or_insert(0);
                    *offset = (*offset).saturating_add(3).min(max_offset);
                }
                return Ok(());
            }
            MouseEventKind::ScrollUp => {
                let offset = self
                    .scroll_offset_per_column
                    .entry(self.focused_column)
                    .or_insert(0);
                *offset = offset.saturating_sub(3);
                return Ok(());
            }
            _ => {}
        }

        if !matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
            return Ok(());
        }

        let x = mouse.column;
        let y = mouse.row;

        if self.active_dialog == ActiveDialog::Help {
            let help_area = Rect {
                x: self.viewport.0.saturating_mul(15) / 100,
                y: self.viewport.1.saturating_mul(10) / 100,
                width: self.viewport.0.saturating_mul(70) / 100,
                height: self.viewport.1.saturating_mul(80) / 100,
            };
            if !point_in_rect(x, y, help_area) {
                self.active_dialog = ActiveDialog::None;
                return Ok(());
            }
        }

        if let Some((_, message)) = self
            .hit_test_map
            .iter()
            .find(|(rect, _)| point_in_rect(x, y, *rect))
        {
            self.update(message.clone())?;
        }

        Ok(())
    }

    fn handle_dialog_key(&mut self, key: KeyEvent) -> Result<()> {
        let mut follow_up: Option<Message> = None;

        match &mut self.active_dialog {
            ActiveDialog::NewTask(state) => {
                let fields = [
                    NewTaskField::Repo,
                    NewTaskField::Branch,
                    NewTaskField::Base,
                    NewTaskField::Title,
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
                    KeyCode::Esc => self.active_dialog = ActiveDialog::None,
                    KeyCode::Tab | KeyCode::Down => {
                        focus_index = move_focus(focus_index, 1);
                        state.focused_field = fields[focus_index].clone();
                    }
                    KeyCode::BackTab | KeyCode::Up => {
                        focus_index = move_focus(focus_index, -1);
                        state.focused_field = fields[focus_index].clone();
                    }
                    KeyCode::Left if state.focused_field == NewTaskField::Repo => {
                        if !self.repos.is_empty() {
                            state.repo_input.clear();
                            state.repo_idx = state.repo_idx.saturating_sub(1);
                            if let Some(repo) = self.repos.get(state.repo_idx) {
                                state.base_input = repo_default_base(repo, &RealCreateTaskRuntime);
                            }
                        }
                    }
                    KeyCode::Right if state.focused_field == NewTaskField::Repo => {
                        if !self.repos.is_empty() {
                            state.repo_input.clear();
                            state.repo_idx = (state.repo_idx + 1).min(self.repos.len() - 1);
                            if let Some(repo) = self.repos.get(state.repo_idx) {
                                state.base_input = repo_default_base(repo, &RealCreateTaskRuntime);
                            }
                        }
                    }
                    KeyCode::Left if state.focused_field == NewTaskField::Create => {
                        state.focused_field = NewTaskField::Cancel;
                    }
                    KeyCode::Right if state.focused_field == NewTaskField::Cancel => {
                        state.focused_field = NewTaskField::Create;
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
                        follow_up = Some(match state.focused_field {
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
            ActiveDialog::CategoryInput(state) => {
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
                    KeyCode::Esc => self.active_dialog = ActiveDialog::None,
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
                        follow_up = Some(match state.focused_field {
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
            ActiveDialog::DeleteCategory(state) => match key.code {
                KeyCode::Esc => self.active_dialog = ActiveDialog::None,
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
                    follow_up = Some(match state.focused_field {
                        DeleteCategoryField::Delete => Message::ConfirmDeleteCategory,
                        DeleteCategoryField::Cancel => Message::DismissDialog,
                    });
                }
                _ => {}
            },
            ActiveDialog::WorktreeNotFound(state) => match key.code {
                KeyCode::Esc => self.active_dialog = ActiveDialog::None,
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
                    follow_up = Some(match state.focused_field {
                        WorktreeNotFoundField::Recreate => Message::WorktreeNotFoundRecreate,
                        WorktreeNotFoundField::MarkBroken => Message::WorktreeNotFoundMarkBroken,
                        WorktreeNotFoundField::Cancel => Message::DismissDialog,
                    });
                }
                _ => {}
            },
            ActiveDialog::RepoUnavailable(_) => {
                if matches!(key.code, KeyCode::Enter | KeyCode::Esc) {
                    follow_up = Some(Message::RepoUnavailableDismiss);
                }
            }
            ActiveDialog::ConfirmQuit(_) => match key.code {
                KeyCode::Esc => follow_up = Some(Message::CancelQuit),
                KeyCode::Left | KeyCode::Char('h') => follow_up = Some(Message::CancelQuit),
                KeyCode::Right | KeyCode::Char('l') => follow_up = Some(Message::ConfirmQuit),
                KeyCode::Enter => follow_up = Some(Message::ConfirmQuit),
                _ => {}
            },
            ActiveDialog::Help => {
                if matches!(key.code, KeyCode::Esc | KeyCode::Char('?')) {
                    self.active_dialog = ActiveDialog::None;
                }
            }
            ActiveDialog::Error(_) => {
                if matches!(key.code, KeyCode::Enter | KeyCode::Esc) {
                    self.active_dialog = ActiveDialog::None;
                }
            }
            _ => {
                if key.code == KeyCode::Esc {
                    self.active_dialog = ActiveDialog::None;
                }
            }
        }

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

    fn selected_task(&self) -> Option<Task> {
        let category = self.categories.get(self.focused_column)?;
        let mut tasks: Vec<Task> = self
            .tasks
            .iter()
            .filter(|task| task.category_id == category.id)
            .cloned()
            .collect();
        tasks.sort_by_key(|task| task.position);

        let selected = self
            .selected_task_per_column
            .get(&self.focused_column)
            .copied()
            .unwrap_or(0);
        tasks.get(selected).cloned()
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
            focused_field: DeleteCategoryField::Cancel,
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
                let created = self.db.add_category(name, next_position)?;
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

        let result = attach_task_with_runtime(&self.db, &task, &repo, &RealRecoveryRuntime)?;
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

        self.db.update_task_tmux(
            task.id,
            None,
            task.opencode_session_id.clone(),
            Some(repo.path),
        )?;
        self.db
            .update_task_status(task.id, Status::Unknown.as_str())?;

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

        let result = create_task_pipeline_with_runtime(
            &self.db,
            &mut self.repos,
            todo_category,
            &dialog_state,
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

    fn active_session_count(&self) -> usize {
        self.tasks
            .iter()
            .filter_map(|task| task.tmux_session_name.as_deref())
            .filter(|session_name| tmux_session_exists(session_name))
            .count()
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
            let _ = handle.join();
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
        session_status: Some(runtime.detect_status(session_name)),
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
            return Status::Unknown.as_str().to_string();
        }
        return current_status.to_string();
    }

    if !observed.session_exists {
        return Status::Dead.as_str().to_string();
    }

    observed
        .session_status
        .unwrap_or(Status::Unknown)
        .as_str()
        .to_string()
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
            debug!(
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
        let observed_status = runtime.detect_status(session_name);
        db.update_task_status(task.id, observed_status.as_str())?;

        if matches!(observed_status, Status::Dead | Status::Unknown) {
            let command = opencode_command(task.opencode_session_id.as_deref());
            runtime.send_command(session_name, &command)?;
            db.update_task_status(task.id, Status::Unknown.as_str())?;
        }

        runtime.switch_client(session_name)?;
        return Ok(AttachTaskResult::Attached);
    }

    let Some(worktree_path) = task.worktree_path.as_deref() else {
        return Ok(AttachTaskResult::WorktreeNotFound);
    };
    let worktree_path = Path::new(worktree_path);
    if !runtime.worktree_exists(worktree_path) {
        return Ok(AttachTaskResult::WorktreeNotFound);
    }

    let session_name = next_available_session_name(
        task.tmux_session_name.as_deref(),
        &repo.name,
        &task.branch,
        runtime,
    );

    let command = if task.tmux_session_name.is_some() {
        opencode_command(task.opencode_session_id.as_deref())
    } else {
        "opencode".to_string()
    };

    runtime.create_session(&session_name, worktree_path, &command)?;
    db.update_task_tmux(
        task.id,
        Some(session_name.clone()),
        task.opencode_session_id.clone(),
        task.worktree_path.clone(),
    )?;
    db.update_task_status(task.id, Status::Unknown.as_str())?;

    Ok(AttachTaskResult::Attached)
}

fn create_task_pipeline_with_runtime(
    db: &Database,
    repos: &mut Vec<Repo>,
    todo_category_id: Uuid,
    state: &NewTaskDialogState,
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
        warn!("{message}");
        warning = Some(message);
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
        let session_name = next_available_session_name_by(None, &repo.name, branch, |name| {
            runtime.tmux_session_exists(name)
        });

        runtime
            .tmux_create_session(&session_name, &worktree_path, Some("opencode"))
            .context("tmux session creation failed")?;
        created_session_name = Some(session_name.clone());

        let task = db
            .add_task(repo.id, branch, state.title_input.trim(), todo_category_id)
            .context("failed to save task")?;
        created_task_id = Some(task.id);

        db.update_task_tmux(
            task.id,
            Some(session_name.clone()),
            None,
            Some(worktree_path.display().to_string()),
        )
        .context("failed to save task runtime metadata")?;
        db.update_task_status(task.id, Status::Unknown.as_str())
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

fn worktrees_root_for_repo(repo_path: &Path) -> PathBuf {
    let parent = repo_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    parent.join(".opencode-kanban-worktrees")
}

fn repo_default_base(repo: &Repo, runtime: &impl CreateTaskRuntime) -> String {
    repo.default_base
        .clone()
        .filter(|base| !base.trim().is_empty())
        .unwrap_or_else(|| runtime.git_detect_default_branch(Path::new(&repo.path)))
}

fn opencode_command(session_id: Option<&str>) -> String {
    match session_id {
        Some(session_id) => format!("opencode -s {session_id}"),
        None => "opencode".to_string(),
    }
}

fn next_available_session_name(
    existing_name: Option<&str>,
    repo_name: &str,
    branch_name: &str,
    runtime: &impl RecoveryRuntime,
) -> String {
    next_available_session_name_by(existing_name, repo_name, branch_name, |name| {
        runtime.session_exists(name)
    })
}

fn next_available_session_name_by<F>(
    existing_name: Option<&str>,
    repo_name: &str,
    branch_name: &str,
    session_exists: F,
) -> String
where
    F: Fn(&str) -> bool,
{
    if let Some(existing_name) = existing_name
        && !session_exists(existing_name)
    {
        return existing_name.to_string();
    }

    let base = sanitize_session_name(repo_name, branch_name);
    if !session_exists(&base) {
        return base;
    }

    for suffix in 2..10_000 {
        let candidate = format!("{base}-{suffix}");
        if !session_exists(&candidate) {
            return candidate;
        }
    }

    base
}

fn spawn_status_poller(db_path: PathBuf, stop: Arc<AtomicBool>) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let runtime = match tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
        {
            Ok(runtime) => runtime,
            Err(_) => return,
        };

        runtime.block_on(async move {
            while !stop.load(Ordering::Relaxed) {
                let db = match Database::open(&db_path) {
                    Ok(db) => db,
                    Err(_) => {
                        tokio::time::sleep(Duration::from_secs(3)).await;
                        continue;
                    }
                };

                let tasks = db.list_tasks().unwrap_or_default();
                if tasks.is_empty() {
                    tokio::time::sleep(Duration::from_secs(3)).await;
                    continue;
                }

                let repos = db.list_repos().unwrap_or_default();
                let repo_paths: HashMap<Uuid, String> =
                    repos.into_iter().map(|repo| (repo.id, repo.path)).collect();

                for (index, task) in tasks.iter().enumerate() {
                    if stop.load(Ordering::Relaxed) {
                        break;
                    }

                    let repo_available = repo_paths
                        .get(&task.repo_id)
                        .map(|path| Path::new(path).exists())
                        .unwrap_or(false);

                    if !repo_available {
                        let _ = db.update_task_status(task.id, STATUS_REPO_UNAVAILABLE);
                        tokio::time::sleep(staggered_poll_delay(index)).await;
                        continue;
                    }

                    if let Some(session_name) = task.tmux_session_name.as_deref() {
                        let status = if tmux_session_exists(session_name) {
                            detect_session_status(session_name)
                        } else {
                            Status::Dead
                        };
                        let _ = db.update_task_status(task.id, status.as_str());
                    }

                    tokio::time::sleep(staggered_poll_delay(index)).await;
                }
            }
        });
    })
}

fn detect_session_status(session_name: &str) -> Status {
    if !opencode_is_running_in_session(session_name) {
        return Status::Dead;
    }

    match tmux_capture_pane(session_name, 50) {
        Ok(pane) => opencode_detect_status(&pane),
        Err(_) => Status::Unknown,
    }
}

pub fn staggered_poll_delay(task_index: usize) -> Duration {
    let base_seconds = 3 + task_index as u64;
    let jitter_ms = current_jitter_ms(task_index);
    Duration::from_secs(base_seconds) + Duration::from_millis(jitter_ms)
}

fn current_jitter_ms(task_index: usize) -> u64 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as u64)
        .unwrap_or(0);
    (nanos + task_index as u64 * 97) % 700
}

fn default_db_path() -> Result<PathBuf> {
    let base = dirs::data_local_dir().unwrap_or_else(|| PathBuf::from("."));
    let app_dir = base.join("opencode-kanban");
    std::fs::create_dir_all(&app_dir)
        .with_context(|| format!("failed to create data dir {}", app_dir.display()))?;
    Ok(app_dir.join("opencode-kanban.sqlite"))
}

fn point_in_rect(x: u16, y: u16, rect: Rect) -> bool {
    x >= rect.x
        && x < rect.x.saturating_add(rect.width)
        && y >= rect.y
        && y < rect.y.saturating_add(rect.height)
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};

    use anyhow::Result;
    use tempfile::TempDir;
    use uuid::Uuid;

    use super::*;

    #[test]
    fn test_staggered_poll_delay_increases_per_task() {
        let one = staggered_poll_delay(0);
        let two = staggered_poll_delay(1);
        assert!(two > one);
    }

    #[test]
    fn test_recovery_startup_with_dead_sessions_updates_status_to_dead() -> Result<()> {
        let fixture = RecoveryFixture::new()?;
        let task = fixture.new_task("startup-dead")?;
        fixture.db.update_task_tmux(
            task.id,
            Some("ok-startup-dead".to_string()),
            Some(Uuid::new_v4().to_string()),
            Some(fixture.worktree().display().to_string()),
        )?;
        fixture
            .db
            .update_task_status(task.id, Status::Running.as_str())?;

        let runtime = FakeRecoveryRuntime::default();
        reconcile_startup_tasks(
            &fixture.db,
            &fixture.db.list_tasks()?,
            &fixture.db.list_repos()?,
            &runtime,
        )?;

        let updated = fixture.db.get_task(task.id)?;
        assert_eq!(updated.tmux_status, Status::Dead.as_str());
        Ok(())
    }

    #[test]
    fn test_recovery_attach_dead_task_with_existing_worktree_recreates_session() -> Result<()> {
        let fixture = RecoveryFixture::new()?;
        let task = fixture.new_task("attach-recreate")?;
        let session_id = Uuid::new_v4().to_string();
        let session_name = "ok-attach-recreate".to_string();

        fixture.db.update_task_tmux(
            task.id,
            Some(session_name.clone()),
            Some(session_id.clone()),
            Some(fixture.worktree().display().to_string()),
        )?;
        fixture
            .db
            .update_task_status(task.id, Status::Dead.as_str())?;

        let runtime = FakeRecoveryRuntime::default();
        let updated_task = fixture.db.get_task(task.id)?;
        let result = attach_task_with_runtime(&fixture.db, &updated_task, &fixture.repo, &runtime)?;

        assert_eq!(result, AttachTaskResult::Attached);
        let created = runtime.created_sessions.borrow();
        assert_eq!(created.len(), 1);
        assert_eq!(created[0].0, session_name);
        assert_eq!(created[0].2, format!("opencode -s {session_id}"));

        let switched = runtime.switched_sessions.borrow();
        assert!(switched.is_empty());
        Ok(())
    }

    #[test]
    fn test_recovery_attach_dead_task_with_missing_worktree_shows_error() -> Result<()> {
        let fixture = RecoveryFixture::new()?;
        let task = fixture.new_task("attach-missing-worktree")?;

        let missing_worktree = fixture.temp.path().join("does-not-exist");
        fixture.db.update_task_tmux(
            task.id,
            Some("ok-attach-missing".to_string()),
            None,
            Some(missing_worktree.display().to_string()),
        )?;
        fixture
            .db
            .update_task_status(task.id, Status::Dead.as_str())?;

        let runtime = FakeRecoveryRuntime::default();
        let updated_task = fixture.db.get_task(task.id)?;
        let result = attach_task_with_runtime(&fixture.db, &updated_task, &fixture.repo, &runtime)?;

        assert_eq!(result, AttachTaskResult::WorktreeNotFound);
        assert!(runtime.created_sessions.borrow().is_empty());
        assert!(runtime.switched_sessions.borrow().is_empty());
        Ok(())
    }

    #[test]
    fn test_create_flow_full_pipeline_with_mock_git_tmux() -> Result<()> {
        let fixture = RecoveryFixture::new()?;
        let mut repos = fixture.db.list_repos()?;
        let runtime = FakeCreateTaskRuntime::default();

        let state = NewTaskDialogState {
            repo_idx: 0,
            repo_input: String::new(),
            branch_input: "feature/create-flow".to_string(),
            base_input: "origin/main".to_string(),
            title_input: "Create flow task".to_string(),
            loading_message: None,
            focused_field: NewTaskField::Create,
        };

        let outcome = create_task_pipeline_with_runtime(
            &fixture.db,
            &mut repos,
            fixture.todo_category,
            &state,
            &runtime,
        )?;

        assert!(outcome.warning.is_none());
        let tasks = fixture.db.list_tasks()?;
        assert_eq!(tasks.len(), 1);

        let task = &tasks[0];
        assert_eq!(task.branch, "feature/create-flow");
        assert_eq!(task.title, "Create flow task");
        assert!(task.worktree_path.is_some());
        assert!(task.tmux_session_name.is_some());

        assert_eq!(runtime.created_worktrees.borrow().len(), 1);
        assert_eq!(runtime.created_sessions.borrow().len(), 1);
        assert!(runtime.switched_sessions.borrow().is_empty());
        Ok(())
    }

    #[test]
    fn test_create_flow_rolls_back_worktree_when_tmux_creation_fails() -> Result<()> {
        let fixture = RecoveryFixture::new()?;
        let mut repos = fixture.db.list_repos()?;
        let runtime = FakeCreateTaskRuntime::default();
        *runtime.fail_tmux_create.borrow_mut() = true;

        let state = NewTaskDialogState {
            repo_idx: 0,
            repo_input: String::new(),
            branch_input: "feature/create-flow-rollback".to_string(),
            base_input: "origin/main".to_string(),
            title_input: String::new(),
            loading_message: None,
            focused_field: NewTaskField::Create,
        };

        let err = create_task_pipeline_with_runtime(
            &fixture.db,
            &mut repos,
            fixture.todo_category,
            &state,
            &runtime,
        )
        .expect_err("create flow should fail when tmux creation fails");
        assert!(format!("{err:#}").contains("tmux session creation failed"));

        assert!(fixture.db.list_tasks()?.is_empty());
        assert_eq!(runtime.created_worktrees.borrow().len(), 1);
        assert_eq!(runtime.removed_worktrees.borrow().len(), 1);
        assert!(runtime.created_sessions.borrow().is_empty());
        Ok(())
    }

    #[derive(Default)]
    struct FakeCreateTaskRuntime {
        fail_fetch: RefCell<bool>,
        fail_tmux_create: RefCell<bool>,
        fail_switch: RefCell<bool>,
        created_worktrees: RefCell<Vec<PathBuf>>,
        removed_worktrees: RefCell<Vec<PathBuf>>,
        sessions: RefCell<HashMap<String, bool>>,
        created_sessions: RefCell<Vec<String>>,
        killed_sessions: RefCell<Vec<String>>,
        switched_sessions: RefCell<Vec<String>>,
    }

    impl CreateTaskRuntime for FakeCreateTaskRuntime {
        fn git_is_valid_repo(&self, _path: &Path) -> bool {
            true
        }

        fn git_detect_default_branch(&self, _repo_path: &Path) -> String {
            "main".to_string()
        }

        fn git_fetch(&self, _repo_path: &Path) -> Result<()> {
            if *self.fail_fetch.borrow() {
                anyhow::bail!("fetch failed")
            }
            Ok(())
        }

        fn git_validate_branch(&self, _repo_path: &Path, branch_name: &str) -> Result<()> {
            if branch_name.contains(' ') {
                anyhow::bail!("invalid branch")
            }
            Ok(())
        }

        fn git_create_worktree(
            &self,
            _repo_path: &Path,
            worktree_path: &Path,
            _branch_name: &str,
            _base_ref: &str,
        ) -> Result<()> {
            self.created_worktrees
                .borrow_mut()
                .push(worktree_path.to_path_buf());
            Ok(())
        }

        fn git_remove_worktree(&self, _repo_path: &Path, worktree_path: &Path) -> Result<()> {
            self.removed_worktrees
                .borrow_mut()
                .push(worktree_path.to_path_buf());
            Ok(())
        }

        fn tmux_session_exists(&self, session_name: &str) -> bool {
            self.sessions
                .borrow()
                .get(session_name)
                .copied()
                .unwrap_or(false)
        }

        fn tmux_create_session(
            &self,
            session_name: &str,
            _working_dir: &Path,
            _command: Option<&str>,
        ) -> Result<()> {
            if *self.fail_tmux_create.borrow() {
                anyhow::bail!("tmux create failed")
            }

            self.sessions
                .borrow_mut()
                .insert(session_name.to_string(), true);
            self.created_sessions
                .borrow_mut()
                .push(session_name.to_string());
            Ok(())
        }

        fn tmux_kill_session(&self, session_name: &str) -> Result<()> {
            self.killed_sessions
                .borrow_mut()
                .push(session_name.to_string());
            self.sessions.borrow_mut().remove(session_name);
            Ok(())
        }
    }

    #[derive(Default)]
    struct FakeRecoveryRuntime {
        repo_paths: RefCell<HashMap<PathBuf, bool>>,
        worktree_paths: RefCell<HashMap<PathBuf, bool>>,
        sessions: RefCell<HashMap<String, Status>>,
        created_sessions: RefCell<Vec<(String, PathBuf, String)>>,
        sent_commands: RefCell<Vec<(String, String)>>,
        switched_sessions: RefCell<Vec<String>>,
    }

    impl RecoveryRuntime for FakeRecoveryRuntime {
        fn repo_exists(&self, repo_path: &Path) -> bool {
            self.repo_paths
                .borrow()
                .get(repo_path)
                .copied()
                .unwrap_or_else(|| repo_path.exists())
        }

        fn worktree_exists(&self, worktree_path: &Path) -> bool {
            self.worktree_paths
                .borrow()
                .get(worktree_path)
                .copied()
                .unwrap_or_else(|| worktree_path.exists())
        }

        fn session_exists(&self, session_name: &str) -> bool {
            self.sessions.borrow().contains_key(session_name)
        }

        fn detect_status(&self, session_name: &str) -> Status {
            self.sessions
                .borrow()
                .get(session_name)
                .copied()
                .unwrap_or(Status::Dead)
        }

        fn create_session(
            &self,
            session_name: &str,
            working_dir: &Path,
            command: &str,
        ) -> Result<()> {
            self.created_sessions.borrow_mut().push((
                session_name.to_string(),
                working_dir.to_path_buf(),
                command.to_string(),
            ));
            self.sessions
                .borrow_mut()
                .insert(session_name.to_string(), Status::Unknown);
            Ok(())
        }

        fn send_command(&self, session_name: &str, command: &str) -> Result<()> {
            self.sent_commands
                .borrow_mut()
                .push((session_name.to_string(), command.to_string()));
            self.sessions
                .borrow_mut()
                .insert(session_name.to_string(), Status::Unknown);
            Ok(())
        }

        fn switch_client(&self, session_name: &str) -> Result<()> {
            self.switched_sessions
                .borrow_mut()
                .push(session_name.to_string());
            Ok(())
        }
    }

    struct RecoveryFixture {
        temp: TempDir,
        db: Database,
        repo: Repo,
        todo_category: Uuid,
    }

    impl RecoveryFixture {
        fn new() -> Result<Self> {
            let temp = TempDir::new()?;
            let db = Database::open(":memory:")?;

            let repo_path = temp.path().join("repo");
            std::fs::create_dir_all(&repo_path)?;
            let repo = db.add_repo(&repo_path)?;

            let todo_category = db.list_categories()?[0].id;
            let worktree = temp.path().join("worktree");
            std::fs::create_dir_all(&worktree)?;

            Ok(Self {
                temp,
                db,
                repo,
                todo_category,
            })
        }

        fn new_task(&self, branch: &str) -> Result<Task> {
            self.db.add_task(
                self.repo.id,
                branch,
                format!("task:{branch}"),
                self.todo_category,
            )
        }

        fn worktree(&self) -> PathBuf {
            self.temp.path().join("worktree")
        }
    }
}
