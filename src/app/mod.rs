pub mod actions;
pub mod dialogs;
pub mod interaction;
pub mod messages;
pub mod polling;
pub mod runtime;
pub mod state;

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use chrono::{DateTime, Local, Utc};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use nucleo::{Config, Matcher, Utf32Str};
use tokio::task::JoinHandle;
use tracing::warn;
use tuirealm::ratatui::layout::Rect;
use tuirealm::ratatui::widgets::{ListState, ScrollbarState};
use uuid::Uuid;

use self::interaction::{InteractionKind, InteractionMap};
pub use self::messages::Message;
pub use self::state::{
    ActiveDialog, ArchiveTaskDialogState, CATEGORY_COLOR_PALETTE, CategoryColorDialogState,
    CategoryColorField, CategoryInputDialogState, CategoryInputField, CategoryInputMode,
    ConfirmCancelField, ConfirmQuitDialogState, ContextMenuItem, ContextMenuState,
    DeleteCategoryDialogState, DeleteProjectDialogState, DeleteRepoDialogState,
    DeleteTaskDialogState, DeleteTaskField, DetailFocus, ErrorDialogState, MoveTaskDialogState,
    NewProjectDialogState, NewProjectField, NewTaskDialogState, NewTaskField,
    RenameProjectDialogState, RenameProjectField, RenameRepoDialogState, RenameRepoField,
    RepoPickerDialogState, RepoSuggestionItem, RepoSuggestionKind, RepoUnavailableDialogState,
    SettingsSection, SettingsViewState, TodoVisualizationMode, View, ViewMode,
    WorktreeNotFoundDialogState, WorktreeNotFoundField, category_color_label,
};

use crate::command_palette::{CommandPaletteState, all_commands};
use crate::db::Database;
use crate::git::{derive_worktree_path, git_delete_branch, git_remove_worktree};
use crate::keybindings::{KeyAction, KeyContext, Keybindings};
use crate::matching::recency_frequency_bonus;
use crate::opencode::{
    OpenCodeServerManager, Status, ensure_server_ready, opencode_attach_command,
};
use crate::projects::{self, ProjectInfo};
use crate::theme::{Theme, ThemePreset};
use crate::tmux::tmux_kill_session;
use crate::types::{
    Category, CommandFrequency, Repo, SessionMessageItem, SessionState, SessionTodoItem, Task,
};

use self::runtime::{
    CreateTaskRuntime, RealCreateTaskRuntime, RealRecoveryRuntime, RecoveryRuntime,
    next_available_session_name, next_available_session_name_by, worktrees_root_for_repo,
};
use self::state::{AttachTaskResult, CreateTaskOutcome, DesiredTaskState, ObservedTaskState};

const REPO_SELECTION_USAGE_PREFIX: &str = "repo-selection:";
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

pub struct ProjectDetailCache {
    pub project_name: String,
    pub task_count: usize,
    pub running_count: usize,
    pub repo_count: usize,
    pub category_count: usize,
    pub file_size_kb: u64,
}

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
    pub archived_tasks: Vec<Task>,
    pub focused_column: usize,
    pub selected_task_per_column: HashMap<usize, usize>,
    pub scroll_offset_per_column: HashMap<usize, usize>,
    pub column_scroll_states: Vec<ScrollbarState>,
    pub active_dialog: ActiveDialog,
    pub footer_notice: Option<String>,
    pub interaction_map: InteractionMap,
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
    poller_thread: Option<JoinHandle<()>>,
    pub view_mode: ViewMode,
    pub side_panel_width: u16,
    pub side_panel_selected_row: usize,
    pub archive_selected_index: usize,
    pub collapsed_categories: HashSet<Uuid>,
    pub current_log_buffer: Option<String>,
    pub detail_focus: DetailFocus,
    pub detail_scroll_offset: usize,
    pub log_scroll_offset: usize,
    pub log_split_ratio: u16,
    pub log_expanded: bool,
    pub log_expanded_scroll_offset: usize,
    pub log_expanded_entries: HashSet<usize>,
    pub session_todo_cache: Arc<Mutex<HashMap<Uuid, Vec<SessionTodoItem>>>>,
    pub session_title_cache: Arc<Mutex<HashMap<String, String>>>,
    pub session_message_cache: Arc<Mutex<HashMap<Uuid, Vec<SessionMessageItem>>>>,
    pub todo_visualization_mode: TodoVisualizationMode,
    pub keybindings: Keybindings,
    pub settings: crate::settings::Settings,
    pub settings_view_state: Option<SettingsViewState>,
    pub category_edit_mode: bool,
    pub project_detail_cache: Option<ProjectDetailCache>,
    last_click: Option<(u16, u16, Instant)>,
    pending_gg_at: Option<Instant>,
}

fn load_project_detail(info: &crate::projects::ProjectInfo) -> Option<ProjectDetailCache> {
    let db = Database::open(&info.path).ok()?;
    let tasks = db.list_tasks().ok()?;
    let repos = db.list_repos().ok()?;
    let categories = db.list_categories().ok()?;
    let running = tasks.iter().filter(|t| t.tmux_status == "running").count();
    let size_kb = fs::metadata(&info.path)
        .ok()
        .map(|m| m.len() / 1024)
        .unwrap_or(0);
    Some(ProjectDetailCache {
        project_name: info.name.clone(),
        task_count: tasks.len(),
        running_count: running,
        repo_count: repos.len(),
        category_count: categories.len(),
        file_size_kb: size_kb,
    })
}

impl App {
    pub fn active_session_count(&self) -> usize {
        self.tasks
            .iter()
            .filter(|t| t.tmux_status == "running")
            .count()
    }

    pub fn new(project_name: Option<&str>) -> Result<Self> {
        Self::new_with_theme(project_name, None)
    }

    pub fn new_with_theme(
        project_name: Option<&str>,
        cli_theme_override: Option<ThemePreset>,
    ) -> Result<Self> {
        let db_path = default_db_path()?;
        let db = Database::open(&db_path)?;
        let server_manager = ensure_server_ready();
        let poller_stop = Arc::new(AtomicBool::new(false));
        let session_todo_cache = Arc::new(Mutex::new(HashMap::new()));
        let session_title_cache = Arc::new(Mutex::new(HashMap::new()));
        let session_message_cache = Arc::new(Mutex::new(HashMap::new()));
        let settings = crate::settings::Settings::load();
        let env_theme = std::env::var("OPENCODE_KANBAN_THEME")
            .ok()
            .and_then(|value| ThemePreset::from_str(&value).ok());
        let settings_theme = ThemePreset::from_str(&settings.theme).ok();
        let effective_theme = cli_theme_override
            .or(env_theme)
            .or(settings_theme)
            .unwrap_or_default();

        let todo_visualization_mode = std::env::var("OPENCODE_KANBAN_TODO_VISUALIZATION")
            .ok()
            .and_then(|value| TodoVisualizationMode::from_str(&value).ok())
            .unwrap_or(TodoVisualizationMode::Checklist);
        let default_view_mode = default_view_mode(&settings);

        let mut app = Self {
            should_quit: false,
            pulse_phase: 0,
            theme: Theme::from_preset(effective_theme),
            layout_epoch: 0,
            viewport: (80, 24),
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
            view_mode: default_view_mode,
            side_panel_width: settings.side_panel_width,
            side_panel_selected_row: 0,
            archive_selected_index: 0,
            collapsed_categories: HashSet::new(),
            current_log_buffer: None,
            detail_focus: DetailFocus::List,
            detail_scroll_offset: 0,
            log_scroll_offset: 0,
            log_split_ratio: 65,
            log_expanded: false,
            log_expanded_scroll_offset: 0,
            log_expanded_entries: HashSet::new(),
            session_todo_cache,
            session_title_cache,
            session_message_cache,
            todo_visualization_mode,
            keybindings: Keybindings::load(),
            settings,
            settings_view_state: None,
            category_edit_mode: false,
            project_detail_cache: None,
            last_click: None,
            pending_gg_at: None,
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
            Arc::clone(&app.session_todo_cache),
            Arc::clone(&app.session_title_cache),
            Arc::clone(&app.session_message_cache),
            app.settings.poll_interval_ms,
        ));
        Ok(app)
    }

    pub fn should_quit(&self) -> bool {
        self.should_quit
    }

    pub fn session_todos(&self, task_id: Uuid) -> Vec<SessionTodoItem> {
        self.session_todo_cache
            .lock()
            .ok()
            .and_then(|cache| cache.get(&task_id).cloned())
            .unwrap_or_default()
    }

    pub fn session_todo_summary(&self, task_id: Uuid) -> Option<(usize, usize)> {
        let todos = self.session_todos(task_id);
        if todos.is_empty() {
            return None;
        }

        let completed = todos.iter().filter(|todo| todo.completed).count();
        Some((completed, todos.len()))
    }

    pub fn opencode_session_title(&self, session_id: &str) -> Option<String> {
        self.session_title_cache
            .lock()
            .ok()
            .and_then(|cache| cache.get(session_id).cloned())
    }

    pub fn session_messages(&self, task_id: Uuid) -> Vec<SessionMessageItem> {
        self.session_message_cache
            .lock()
            .ok()
            .and_then(|cache| cache.get(&task_id).cloned())
            .unwrap_or_default()
    }

    fn build_log_buffer_from_messages(messages: &[SessionMessageItem]) -> Option<String> {
        let mut lines = Vec::new();

        for message in messages.iter().rev() {
            let content = message.content.trim();
            if content.is_empty() {
                continue;
            }

            let kind = log_kind_label(message.message_type.as_deref());
            let role = log_role_label(message.role.as_deref());
            let timestamp = log_time_label(message.timestamp.as_deref());

            lines.push(format!("> [{kind}] {role:<9} {timestamp}"));

            for line in content.lines() {
                let trimmed = line.trim_end();
                if trimmed.is_empty() {
                    continue;
                }
                lines.push(format!("  {trimmed}"));
            }

            lines.push(String::new());
        }

        while matches!(lines.last(), Some(last) if last.is_empty()) {
            lines.pop();
        }

        let output = lines.join("\n");

        if output.is_empty() {
            None
        } else {
            Some(output)
        }
    }

    fn poller_db_path(&self) -> PathBuf {
        self.current_project_path
            .clone()
            .unwrap_or_else(|| projects::get_project_path(projects::DEFAULT_PROJECT))
    }

    fn restart_status_poller(&mut self) {
        self.poller_stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.poller_thread.take() {
            handle.abort();
        }

        self.poller_stop.store(false, Ordering::Relaxed);
        self.poller_thread = Some(polling::spawn_status_poller(
            self.poller_db_path(),
            Arc::clone(&self.poller_stop),
            Arc::clone(&self.session_todo_cache),
            Arc::clone(&self.session_title_cache),
            Arc::clone(&self.session_message_cache),
            self.settings.poll_interval_ms,
        ));
    }

    fn save_settings_with_notice(&mut self) {
        match self.settings.save() {
            Ok(()) => {
                self.footer_notice = Some("  ✓ Settings saved  ".to_string());
            }
            Err(err) => {
                warn!(error = %err, "failed to save settings");
                self.footer_notice = Some(" Failed to save settings to disk ".to_string());
            }
        }
    }

    pub fn refresh_data(&mut self) -> Result<()> {
        self.tasks = self.db.list_tasks().context("failed to load tasks")?;
        self.categories = self
            .db
            .list_categories()
            .context("failed to load categories")?;
        self.repos = self.db.list_repos().context("failed to load repos")?;

        if let Ok(mut cache) = self.session_todo_cache.lock() {
            cache.retain(|task_id, _| self.tasks.iter().any(|task| task.id == *task_id));
        }
        if let Ok(mut cache) = self.session_message_cache.lock() {
            cache.retain(|task_id, _| self.tasks.iter().any(|task| task.id == *task_id));
        }

        self.collapsed_categories.retain(|category_id| {
            self.categories
                .iter()
                .any(|category| category.id == *category_id)
        });

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
        } else {
            self.column_scroll_states.clear();
            self.focused_column = 0;
            self.side_panel_selected_row = 0;
            self.detail_focus = DetailFocus::List;
            self.detail_scroll_offset = 0;
            self.log_scroll_offset = 0;
            self.log_expanded = false;
            self.log_expanded_scroll_offset = 0;
            self.log_expanded_entries.clear();
        }

        if self.view_mode == ViewMode::SidePanel {
            let rows = self.side_panel_rows();
            self.sync_side_panel_selection(&rows, rows.is_empty());
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
            if let Some(project) = self.project_list.get(self.selected_project_index) {
                self.project_detail_cache = load_project_detail(project);
            }
        } else {
            self.selected_project_index = 0;
            self.project_list_state.select(None);
            self.project_detail_cache = None;
        }
        Ok(())
    }

    pub fn switch_project(&mut self, path: PathBuf) -> Result<()> {
        self.poller_stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.poller_thread.take() {
            handle.abort();
        }

        let db = Database::open(&path)?;
        self.db = db;
        if let Ok(mut cache) = self.session_todo_cache.lock() {
            cache.clear();
        }
        if let Ok(mut cache) = self.session_title_cache.lock() {
            cache.clear();
        }
        if let Ok(mut cache) = self.session_message_cache.lock() {
            cache.clear();
        }
        self.log_expanded_entries.clear();
        self.refresh_data()?;

        self.poller_stop.store(false, Ordering::Relaxed);
        self.poller_thread = Some(polling::spawn_status_poller(
            path.clone(),
            Arc::clone(&self.poller_stop),
            Arc::clone(&self.session_todo_cache),
            Arc::clone(&self.session_title_cache),
            Arc::clone(&self.session_message_cache),
            self.settings.poll_interval_ms,
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

                    if task.opencode_session_id.is_none() {
                        self.current_log_buffer = None;
                    } else {
                        let messages = self.session_messages(task.id);
                        self.current_log_buffer = Self::build_log_buffer_from_messages(&messages);
                    }
                }
            }
            Message::Resize(w, h) => {
                self.viewport = (w, h);
                self.layout_epoch = self.layout_epoch.saturating_add(1);
                self.interaction_map.clear();
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
                let usage = repo_selection_usage_map(&self.db);
                let ranked_repo_indexes = rank_repos_for_query("", &self.repos, &usage);
                let preferred_repo_idx = ranked_repo_indexes.first().copied().unwrap_or(0);
                let default_base = self
                    .repos
                    .get(preferred_repo_idx)
                    .and_then(|repo| repo.default_base.clone())
                    .unwrap_or_else(|| "main".to_string());
                self.active_dialog = ActiveDialog::NewTask(NewTaskDialogState {
                    repo_idx: preferred_repo_idx,
                    repo_input: String::new(),
                    repo_picker: None,
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
                self.archived_tasks.clear();
                self.archive_selected_index = 0;
                self.active_dialog = ActiveDialog::None;
            }
            Message::OpenSettings => {
                self.settings_view_state = Some(SettingsViewState {
                    active_section: SettingsSection::General,
                    general_selected_field: 0,
                    category_color_selected: self
                        .focused_column
                        .min(self.categories.len().saturating_sub(1)),
                    repos_selected_field: 0,
                    previous_view: self.current_view,
                });
                self.current_view = View::Settings;
                self.active_dialog = ActiveDialog::None;
                self.context_menu = None;
                self.hovered_message = None;
            }
            Message::OpenArchiveView => {
                self.archived_tasks = self.db.list_archived_tasks()?;
                self.archive_selected_index = 0;
                self.current_view = View::Archive;
                self.active_dialog = ActiveDialog::None;
                self.context_menu = None;
                self.hovered_message = None;
            }
            Message::CloseArchiveView => {
                self.current_view = View::Board;
                self.archived_tasks.clear();
                self.archive_selected_index = 0;
                self.active_dialog = ActiveDialog::None;
            }
            Message::CloseSettings => {
                if let Some(state) = self.settings_view_state.take() {
                    self.current_view = state.previous_view;
                } else {
                    self.current_view = View::Board;
                }
            }
            Message::SettingsNextSection => {
                if let Some(state) = &mut self.settings_view_state {
                    state.active_section = match state.active_section {
                        SettingsSection::General => SettingsSection::CategoryColors,
                        SettingsSection::CategoryColors => SettingsSection::Keybindings,
                        SettingsSection::Keybindings => SettingsSection::Repos,
                        SettingsSection::Repos => SettingsSection::General,
                    };
                }
            }
            Message::SettingsPrevSection => {
                if let Some(state) = &mut self.settings_view_state {
                    state.active_section = match state.active_section {
                        SettingsSection::General => SettingsSection::Repos,
                        SettingsSection::CategoryColors => SettingsSection::General,
                        SettingsSection::Keybindings => SettingsSection::CategoryColors,
                        SettingsSection::Repos => SettingsSection::Keybindings,
                    };
                }
            }
            Message::SettingsNextItem => {
                if let Some(state) = &mut self.settings_view_state {
                    match state.active_section {
                        SettingsSection::General => {
                            state.general_selected_field =
                                state.general_selected_field.saturating_add(1).min(3);
                        }
                        SettingsSection::CategoryColors => {
                            state.category_color_selected = state
                                .category_color_selected
                                .saturating_add(1)
                                .min(self.categories.len().saturating_sub(1));
                        }
                        SettingsSection::Repos => {
                            state.repos_selected_field = state
                                .repos_selected_field
                                .saturating_add(1)
                                .min(self.repos.len().saturating_sub(1));
                        }
                        SettingsSection::Keybindings => {}
                    }
                }
            }
            Message::SettingsPrevItem => {
                if let Some(state) = &mut self.settings_view_state {
                    match state.active_section {
                        SettingsSection::General => {
                            state.general_selected_field =
                                state.general_selected_field.saturating_sub(1);
                        }
                        SettingsSection::CategoryColors => {
                            state.category_color_selected =
                                state.category_color_selected.saturating_sub(1);
                        }
                        SettingsSection::Repos => {
                            state.repos_selected_field =
                                state.repos_selected_field.saturating_sub(1);
                        }
                        SettingsSection::Keybindings => {}
                    }
                }
            }
            Message::SettingsToggle => {
                if let Some(state) = &self.settings_view_state {
                    match state.active_section {
                        SettingsSection::General => {
                            match state.general_selected_field {
                                0 => {
                                    self.settings.theme = match self.settings.theme.as_str() {
                                        "default" => "high-contrast".to_string(),
                                        "high-contrast" => "mono".to_string(),
                                        _ => "default".to_string(),
                                    };
                                    let theme_preset = ThemePreset::from_str(&self.settings.theme)
                                        .unwrap_or(ThemePreset::Default);
                                    self.theme = Theme::from_preset(theme_preset);
                                }
                                1 => {
                                    let next = self.settings.poll_interval_ms.saturating_add(500);
                                    self.settings.poll_interval_ms =
                                        if next > 30_000 { 500 } else { next };
                                    self.restart_status_poller();
                                }
                                2 => {
                                    let next = self.settings.side_panel_width.saturating_add(5);
                                    self.settings.side_panel_width =
                                        if next > 80 { 20 } else { next };
                                    self.side_panel_width = self.settings.side_panel_width;
                                }
                                3 => {
                                    self.settings.default_view =
                                        if self.settings.default_view == "kanban" {
                                            "detail".to_string()
                                        } else {
                                            "kanban".to_string()
                                        };
                                }
                                _ => {}
                            }
                            self.save_settings_with_notice();
                        }
                        SettingsSection::CategoryColors => {
                            let Some((category_id, current_color)) = self
                                .categories
                                .get(
                                    state
                                        .category_color_selected
                                        .min(self.categories.len().saturating_sub(1)),
                                )
                                .map(|category| (category.id, category.color.clone()))
                            else {
                                return Ok(());
                            };

                            let next_color = next_palette_color(current_color.as_deref());
                            self.db
                                .update_category_color(category_id, next_color)
                                .context("failed to update category color")?;
                            self.refresh_data()?;

                            if let Some(state) = &mut self.settings_view_state {
                                state.category_color_selected = self
                                    .categories
                                    .iter()
                                    .position(|category| category.id == category_id)
                                    .unwrap_or_else(|| {
                                        state
                                            .category_color_selected
                                            .min(self.categories.len().saturating_sub(1))
                                    });
                            }
                        }
                        SettingsSection::Keybindings => {}
                        SettingsSection::Repos => {}
                    }
                }
            }
            Message::SettingsDecreaseItem => {
                if let Some(state) = &self.settings_view_state
                    && state.active_section == SettingsSection::General
                {
                    match state.general_selected_field {
                        0 => {
                            self.settings.theme = match self.settings.theme.as_str() {
                                "high-contrast" => "default".to_string(),
                                "mono" => "high-contrast".to_string(),
                                _ => "mono".to_string(),
                            };
                            let theme_preset = ThemePreset::from_str(&self.settings.theme)
                                .unwrap_or(ThemePreset::Default);
                            self.theme = Theme::from_preset(theme_preset);
                        }
                        1 => {
                            let prev = self.settings.poll_interval_ms.saturating_sub(500);
                            self.settings.poll_interval_ms = if prev < 500 { 30_000 } else { prev };
                            self.restart_status_poller();
                        }
                        2 => {
                            let prev = self.settings.side_panel_width.saturating_sub(5);
                            self.settings.side_panel_width = if prev < 20 { 80 } else { prev };
                            self.side_panel_width = self.settings.side_panel_width;
                        }
                        _ => {}
                    }
                    self.save_settings_with_notice();
                }
            }
            Message::SettingsResetItem => {
                if let Some(state) = &self.settings_view_state
                    && state.active_section == SettingsSection::General
                {
                    match state.general_selected_field {
                        0 => {
                            self.settings.theme = "default".to_string();
                            self.theme = Theme::from_preset(ThemePreset::Default);
                        }
                        1 => {
                            self.settings.poll_interval_ms = 1_000;
                            self.restart_status_poller();
                        }
                        2 => {
                            self.settings.side_panel_width = 40;
                            self.side_panel_width = 40;
                        }
                        _ => {}
                    }
                    self.save_settings_with_notice();
                }
            }
            Message::SettingsSelectSection(section) => {
                if let Some(state) = &mut self.settings_view_state {
                    state.active_section = section;
                }
            }
            Message::SettingsSelectGeneralField(index) => {
                if let Some(state) = &mut self.settings_view_state {
                    state.active_section = SettingsSection::General;
                    state.general_selected_field = index.min(3);
                }
            }
            Message::SettingsSelectCategoryColor(index) => {
                if let Some(state) = &mut self.settings_view_state {
                    state.active_section = SettingsSection::CategoryColors;
                    state.category_color_selected =
                        index.min(self.categories.len().saturating_sub(1));
                }
            }
            Message::SettingsSelectRepo(index) => {
                if let Some(state) = &mut self.settings_view_state {
                    state.active_section = SettingsSection::Repos;
                    state.repos_selected_field = index.min(self.repos.len().saturating_sub(1));
                }
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
                let rows = self.side_panel_rows();
                self.sync_side_panel_selection_at(&rows, index, true);
                self.detail_focus = DetailFocus::List;
            }
            Message::FocusSidePanel(focus) => {
                self.detail_focus = focus;
            }
            Message::ToggleSidePanelCategoryCollapse => self.toggle_side_panel_category_collapse(),
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
            Message::OpenArchiveTaskDialog => self.open_archive_task_dialog()?,
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
                    "settings" => {
                        self.update(Message::OpenSettings)?;
                    }
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
                if let Some(category) = self.categories.get(col_idx) {
                    let next_color = next_palette_color(category.color.as_deref());
                    self.db
                        .update_category_color(category.id, next_color)
                        .context("failed to update category color")?;
                    self.refresh_data()?;
                }
            }
            Message::OpenCategoryColorDialog => self.open_category_color_dialog(),
            Message::ConfirmCategoryColor => self.confirm_category_color()?,
            Message::CycleTodoVisualization => {
                self.todo_visualization_mode = self.todo_visualization_mode.cycle();
            }
            Message::DeleteTaskToggleKillTmux
            | Message::DeleteTaskToggleRemoveWorktree
            | Message::DeleteTaskToggleDeleteBranch => {}
            Message::ConfirmDeleteTask => self.confirm_delete_task()?,
            Message::ConfirmArchiveTask => self.confirm_archive_task()?,
            Message::UnarchiveTask => self.unarchive_selected_task()?,
            Message::ArchiveSelectUp => {
                self.archive_selected_index = self.archive_selected_index.saturating_sub(1);
            }
            Message::ArchiveSelectDown => {
                let max = self.archived_tasks.len().saturating_sub(1);
                self.archive_selected_index = (self.archive_selected_index + 1).min(max);
            }
            Message::SwitchToProjectList => {
                self.current_view = View::ProjectList;
                self.archived_tasks.clear();
                self.archive_selected_index = 0;
            }
            Message::SwitchToBoard(path) => {
                self.switch_project(path)?;
                self.current_view = View::Board;
                self.archived_tasks.clear();
                self.archive_selected_index = 0;
            }
            Message::ProjectListSelectUp => {
                if self.selected_project_index > 0 {
                    self.selected_project_index -= 1;
                    self.project_list_state
                        .select(Some(self.selected_project_index));
                    if let Some(project) = self.project_list.get(self.selected_project_index) {
                        self.project_detail_cache = load_project_detail(project);
                    }
                }
            }
            Message::ProjectListSelectDown => {
                if self.selected_project_index + 1 < self.project_list.len() {
                    self.selected_project_index += 1;
                    self.project_list_state
                        .select(Some(self.selected_project_index));
                    if let Some(project) = self.project_list.get(self.selected_project_index) {
                        self.project_detail_cache = load_project_detail(project);
                    }
                }
            }
            Message::ProjectListConfirm => {
                if let Some(project) = self.project_list.get(self.selected_project_index) {
                    self.switch_project(project.path.clone())?;
                    self.current_view = View::Board;
                    self.archived_tasks.clear();
                    self.archive_selected_index = 0;
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
                                self.archived_tasks.clear();
                                self.archive_selected_index = 0;
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
            Message::OpenRenameProjectDialog => {
                if let Some(project) = self.project_list.get(self.selected_project_index) {
                    self.active_dialog = ActiveDialog::RenameProject(RenameProjectDialogState {
                        name_input: project.name.clone(),
                        focused_field: RenameProjectField::Name,
                    });
                }
            }
            Message::ConfirmRenameProject => {
                if let ActiveDialog::RenameProject(state) = &self.active_dialog {
                    let new_name = state.name_input.trim().to_string();
                    if !new_name.is_empty()
                        && let Some(project) = self.project_list.get(self.selected_project_index)
                    {
                        let old_path = project.path.clone();
                        let is_current = self.current_project_path.as_deref() == Some(&old_path);
                        match projects::rename_project(&old_path, &new_name) {
                            Ok(new_path) => {
                                self.active_dialog = ActiveDialog::None;
                                if is_current {
                                    self.current_project_path = Some(new_path.clone());
                                }
                                self.refresh_projects()?;
                            }
                            Err(e) => {
                                self.active_dialog = ActiveDialog::Error(ErrorDialogState {
                                    title: "Failed to rename project".to_string(),
                                    detail: e.to_string(),
                                });
                            }
                        }
                    }
                }
            }
            Message::FocusRenameProjectField(field) => {
                if let ActiveDialog::RenameProject(state) = &mut self.active_dialog {
                    state.focused_field = field;
                }
            }
            Message::OpenDeleteProjectDialog => {
                if let Some(project) = self.project_list.get(self.selected_project_index) {
                    self.active_dialog = ActiveDialog::DeleteProject(DeleteProjectDialogState {
                        project_name: project.name.clone(),
                        project_path: project.path.clone(),
                    });
                }
            }
            Message::ConfirmDeleteProject => {
                if let ActiveDialog::DeleteProject(state) = &self.active_dialog {
                    let path = state.project_path.clone();
                    let is_current = self.current_project_path.as_deref() == Some(&path);
                    if is_current {
                        self.active_dialog = ActiveDialog::Error(ErrorDialogState {
                            title: "Cannot delete active project".to_string(),
                            detail: "Switch to another project first.".to_string(),
                        });
                    } else {
                        match projects::delete_project(&path) {
                            Ok(()) => {
                                self.active_dialog = ActiveDialog::None;
                                self.selected_project_index =
                                    self.selected_project_index.saturating_sub(1);
                                self.refresh_projects()?;
                            }
                            Err(e) => {
                                self.active_dialog = ActiveDialog::Error(ErrorDialogState {
                                    title: "Failed to delete project".to_string(),
                                    detail: e.to_string(),
                                });
                            }
                        }
                    }
                }
            }
            Message::OpenRenameRepoDialog => {
                let repos_selected = self
                    .settings_view_state
                    .as_ref()
                    .map(|s| s.repos_selected_field)
                    .unwrap_or(0);
                if let Some(repo) = self.repos.get(repos_selected) {
                    self.active_dialog = ActiveDialog::RenameRepo(RenameRepoDialogState {
                        repo_id: repo.id,
                        name_input: repo.name.clone(),
                        focused_field: RenameRepoField::Name,
                    });
                }
            }
            Message::ConfirmRenameRepo => {
                if let ActiveDialog::RenameRepo(state) = &self.active_dialog {
                    let new_name = state.name_input.trim().to_string();
                    let repo_id = state.repo_id;
                    if !new_name.is_empty() {
                        match self.db.update_repo_name(repo_id, &new_name) {
                            Ok(()) => {
                                self.active_dialog = ActiveDialog::None;
                                self.refresh_data()?;
                            }
                            Err(e) => {
                                self.active_dialog = ActiveDialog::Error(ErrorDialogState {
                                    title: "Failed to rename repo".to_string(),
                                    detail: e.to_string(),
                                });
                            }
                        }
                    }
                }
            }
            Message::FocusRenameRepoField(field) => {
                if let ActiveDialog::RenameRepo(state) = &mut self.active_dialog {
                    state.focused_field = field;
                }
            }
            Message::OpenDeleteRepoDialog => {
                let repos_selected = self
                    .settings_view_state
                    .as_ref()
                    .map(|s| s.repos_selected_field)
                    .unwrap_or(0);
                if let Some(repo) = self.repos.get(repos_selected) {
                    self.active_dialog = ActiveDialog::DeleteRepo(DeleteRepoDialogState {
                        repo_id: repo.id,
                        repo_name: repo.name.clone(),
                    });
                }
            }
            Message::ConfirmDeleteRepo => {
                if let ActiveDialog::DeleteRepo(state) = &self.active_dialog {
                    let repo_id = state.repo_id;
                    match self.db.delete_repo(repo_id) {
                        Ok(()) => {
                            self.active_dialog = ActiveDialog::None;
                            self.refresh_data()?;
                            if let Some(s) = &mut self.settings_view_state {
                                s.repos_selected_field = s.repos_selected_field.saturating_sub(1);
                            }
                        }
                        Err(e) => {
                            self.active_dialog = ActiveDialog::Error(ErrorDialogState {
                                title: "Failed to delete repo".to_string(),
                                detail: e.to_string(),
                            });
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
                        self.archived_tasks.clear();
                        self.archive_selected_index = 0;
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
            Message::ToggleCategoryEditMode => {}
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

        if let Some(started_at) = self.pending_gg_at
            && started_at.elapsed() > GG_SEQUENCE_TIMEOUT
        {
            self.pending_gg_at = None;
        }

        if self.current_view == View::Board && !self.log_expanded {
            if key.modifiers == KeyModifiers::empty() && key.code == KeyCode::Char('g') {
                if let Some(started_at) = self.pending_gg_at
                    && started_at.elapsed() <= GG_SEQUENCE_TIMEOUT
                {
                    self.pending_gg_at = None;
                    self.move_selection_to_top();
                } else {
                    self.pending_gg_at = Some(Instant::now());
                }
                return Ok(());
            }

            self.pending_gg_at = None;
        } else {
            self.pending_gg_at = None;
        }

        if self.log_expanded {
            match key.code {
                KeyCode::Esc | KeyCode::Char('f') => {
                    self.log_expanded = false;
                    self.log_scroll_offset = self.log_expanded_scroll_offset;
                }
                KeyCode::Enter | KeyCode::Char('e') => self.toggle_selected_log_entry(true),
                KeyCode::Down | KeyCode::Char('j') => self.scroll_expanded_log_down(1),
                KeyCode::Up | KeyCode::Char('k') => self.scroll_expanded_log_up(1),
                KeyCode::PageDown => self.scroll_expanded_log_down(10),
                KeyCode::PageUp => self.scroll_expanded_log_up(10),
                _ => {}
            }
            return Ok(());
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
                    self.log_expanded = false;
                    self.log_expanded_scroll_offset = 0;
                    self.log_expanded_entries.clear();

                    match self.view_mode {
                        ViewMode::Kanban => {
                            self.view_mode = ViewMode::SidePanel;
                            self.detail_focus = DetailFocus::List;
                            self.detail_scroll_offset = 0;
                            self.log_scroll_offset = 0;

                            let rows = self.side_panel_rows();
                            let current_id = self
                                .selected_task_in_column(self.focused_column)
                                .map(|task| task.id);
                            let index = current_id
                                .and_then(|id| {
                                    rows.iter().position(|row| {
                                        matches!(row, SidePanelRow::Task { task, .. } if task.id == id)
                                    })
                                })
                                .or_else(|| {
                                    rows.iter().position(|row| {
                                        matches!(row, SidePanelRow::CategoryHeader { .. })
                                    })
                                })
                                .unwrap_or(0);
                            self.sync_side_panel_selection_at(&rows, index, false);
                        }
                        ViewMode::SidePanel => {
                            self.view_mode = ViewMode::Kanban;
                            self.detail_focus = DetailFocus::List;
                        }
                    }
                }
                KeyAction::ShrinkPanel => {
                    self.side_panel_width = self.side_panel_width.saturating_sub(5).max(20);
                }
                KeyAction::ExpandPanel => {
                    self.side_panel_width = self.side_panel_width.saturating_add(5).min(80);
                }
                KeyAction::OpenArchiveView => {
                    self.update(Message::OpenArchiveView)?;
                }
                _ => {}
            }
            return Ok(());
        }

        if self.current_view == View::Board
            && self.view_mode == ViewMode::SidePanel
            && key.code == KeyCode::Char(' ')
            && key.modifiers == KeyModifiers::empty()
        {
            self.update(Message::ToggleSidePanelCategoryCollapse)?;
            return Ok(());
        }

        if self.current_view == View::Board && self.view_mode == ViewMode::SidePanel {
            match key.code {
                KeyCode::Tab => {
                    self.cycle_detail_focus();
                    return Ok(());
                }
                KeyCode::Enter | KeyCode::Char('e') => {
                    if self.detail_focus == DetailFocus::Log {
                        self.toggle_selected_log_entry(false);
                        return Ok(());
                    }
                }
                KeyCode::Char('f') => {
                    if self.detail_focus == DetailFocus::Log {
                        self.log_expanded = !self.log_expanded;
                        self.log_expanded_scroll_offset = self.log_scroll_offset;
                    }
                    return Ok(());
                }
                KeyCode::Char('+') | KeyCode::Char('=') => {
                    if self.detail_focus != DetailFocus::List {
                        self.log_split_ratio = self.log_split_ratio.saturating_sub(5).max(35);
                    }
                    return Ok(());
                }
                KeyCode::Char('-') => {
                    if self.detail_focus != DetailFocus::List {
                        self.log_split_ratio = self.log_split_ratio.saturating_add(5).min(80);
                    }
                    return Ok(());
                }
                _ => {}
            }
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
                    KeyAction::ProjectRename => self.update(Message::OpenRenameProjectDialog)?,
                    KeyAction::ProjectDelete => self.update(Message::OpenDeleteProjectDialog)?,
                    _ => {}
                }
            }
            return Ok(());
        }

        if self.current_view == View::Settings {
            let active_section = self.settings_view_state.as_ref().map(|s| s.active_section);
            let msg = match key.code {
                KeyCode::Tab | KeyCode::Right | KeyCode::Char('l') => {
                    Some(Message::SettingsNextSection)
                }
                KeyCode::BackTab | KeyCode::Left | KeyCode::Char('h') => {
                    Some(Message::SettingsPrevSection)
                }
                KeyCode::Up | KeyCode::Char('k') => Some(Message::SettingsPrevItem),
                KeyCode::Down | KeyCode::Char('j') => Some(Message::SettingsNextItem),
                KeyCode::Enter | KeyCode::Char(' ') => Some(Message::SettingsToggle),
                KeyCode::Char('r') if active_section == Some(SettingsSection::Repos) => {
                    Some(Message::OpenRenameRepoDialog)
                }
                KeyCode::Char('x') if active_section == Some(SettingsSection::Repos) => {
                    Some(Message::OpenDeleteRepoDialog)
                }
                KeyCode::Char('0') if active_section == Some(SettingsSection::General) => {
                    Some(Message::SettingsResetItem)
                }
                KeyCode::Esc => Some(Message::CloseSettings),
                _ => None,
            };

            let msg = if active_section == Some(SettingsSection::General) {
                match key.code {
                    KeyCode::Right | KeyCode::Char('l') => Some(Message::SettingsToggle),
                    KeyCode::Left | KeyCode::Char('h') => Some(Message::SettingsDecreaseItem),
                    _ => msg,
                }
            } else {
                msg
            };

            if let Some(msg) = msg {
                self.update(msg)?;
            }
            return Ok(());
        }

        if self.current_view == View::Archive {
            match key.code {
                KeyCode::Up | KeyCode::Char('k') => self.update(Message::ArchiveSelectUp)?,
                KeyCode::Down | KeyCode::Char('j') => self.update(Message::ArchiveSelectDown)?,
                KeyCode::Char('u') => self.update(Message::UnarchiveTask)?,
                KeyCode::Char('d') => self.update(Message::OpenDeleteTaskDialog)?,
                KeyCode::Esc => self.update(Message::CloseArchiveView)?,
                _ => {}
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
                        match self.detail_focus {
                            DetailFocus::List => {
                                let rows = self.side_panel_rows();
                                if rows.is_empty() {
                                    self.side_panel_selected_row = 0;
                                    self.current_log_buffer = None;
                                } else {
                                    let current = self.side_panel_selected_row.min(rows.len() - 1);
                                    let next = (current + 1) % rows.len();
                                    self.sync_side_panel_selection_at(&rows, next, true);
                                }
                            }
                            DetailFocus::Details => self.scroll_details_down(1),
                            DetailFocus::Log => self.scroll_log_down(1),
                        }
                    } else {
                        self.update(Message::SelectDown)?;
                    }
                }
                KeyAction::SelectUp => {
                    if self.view_mode == ViewMode::SidePanel {
                        match self.detail_focus {
                            DetailFocus::List => {
                                let rows = self.side_panel_rows();
                                if rows.is_empty() {
                                    self.side_panel_selected_row = 0;
                                    self.current_log_buffer = None;
                                } else {
                                    let current = self.side_panel_selected_row.min(rows.len() - 1);
                                    let prev = if current == 0 {
                                        rows.len() - 1
                                    } else {
                                        current - 1
                                    };
                                    self.sync_side_panel_selection_at(&rows, prev, true);
                                }
                            }
                            DetailFocus::Details => self.scroll_details_up(1),
                            DetailFocus::Log => self.scroll_log_up(1),
                        }
                    } else {
                        self.update(Message::SelectUp)?;
                    }
                }
                KeyAction::SelectHalfPageDown => {
                    self.move_selection_half_page_down();
                }
                KeyAction::SelectHalfPageUp => {
                    self.move_selection_half_page_up();
                }
                KeyAction::SelectBottom => {
                    self.move_selection_to_bottom();
                }
                KeyAction::NewTask => {
                    self.update(Message::OpenNewTaskDialog)?;
                }
                KeyAction::AddCategory => {
                    self.update(Message::OpenAddCategoryDialog)?;
                }
                KeyAction::CycleCategoryColor => {
                    if self.category_edit_mode {
                        self.update(Message::OpenCategoryColorDialog)?;
                    } else {
                        self.update(Message::CycleCategoryColor(self.focused_column))?;
                    }
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
                KeyAction::ArchiveTask => {
                    self.update(Message::OpenArchiveTaskDialog)?;
                }
                KeyAction::MoveTaskLeft => {
                    if self.category_edit_mode {
                        self.move_category_left()?;
                    } else {
                        self.update(Message::MoveTaskLeft)?;
                    }
                }
                KeyAction::MoveTaskRight => {
                    if self.category_edit_mode {
                        self.move_category_right()?;
                    } else {
                        self.update(Message::MoveTaskRight)?;
                    }
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
                KeyAction::CycleTodoVisualization => {
                    self.update(Message::CycleTodoVisualization)?;
                }
                KeyAction::Dismiss => {
                    if self.view_mode == ViewMode::SidePanel
                        && self.current_view == View::Board
                        && self.detail_focus != DetailFocus::List
                    {
                        self.detail_focus = DetailFocus::List;
                    } else {
                        self.update(Message::DismissDialog)?;
                    }
                }
                KeyAction::ToggleCategoryEditMode => {
                    if self.active_dialog == ActiveDialog::None {
                        self.category_edit_mode = !self.category_edit_mode;
                    }
                }
                _ => {}
            }
        }

        Ok(())
    }

    fn handle_mouse(&mut self, mouse: MouseEvent) -> Result<()> {
        self.mouse_seen = true;
        self.last_mouse_event = Some(mouse);

        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                self.hovered_message = None;

                if let Some((lc, lr, lt)) = self.last_click
                    && lc == mouse.column
                    && lr == mouse.row
                    && lt.elapsed() < Duration::from_millis(400)
                {
                    self.last_click = None;
                    return self.update(Message::AttachSelectedTask);
                }
                self.last_click = Some((mouse.column, mouse.row, Instant::now()));

                let hit = self.interaction_map.resolve_message(
                    mouse.column,
                    mouse.row,
                    InteractionKind::LeftClick,
                );

                if let Some(msg) = hit {
                    self.context_menu = None;
                    self.update(msg)?;
                }
            }

            MouseEventKind::Down(MouseButton::Right) => {
                let mut found_task = false;
                if let Some(Message::SelectTask(col, task_idx)) = self
                    .interaction_map
                    .resolve_message(mouse.column, mouse.row, InteractionKind::RightClick)
                {
                    let category = self.categories.get(col);
                    if let Some(category) = category {
                        let mut tasks: Vec<Task> = self
                            .tasks
                            .iter()
                            .filter(|t| t.category_id == category.id)
                            .cloned()
                            .collect();
                        tasks.sort_by_key(|t| t.position);
                        if let Some(task) = tasks.get(task_idx) {
                            self.context_menu = Some(ContextMenuState {
                                position: (mouse.column, mouse.row),
                                task_id: task.id,
                                task_column: col,
                                items: vec![
                                    ContextMenuItem::Attach,
                                    ContextMenuItem::Delete,
                                    ContextMenuItem::Move,
                                ],
                                selected_index: 0,
                            });
                            found_task = true;
                        }
                    }
                }
                if !found_task {
                    self.context_menu = None;
                }
            }

            MouseEventKind::Moved => {
                let hit = self.interaction_map.resolve_message(
                    mouse.column,
                    mouse.row,
                    InteractionKind::Hover,
                );
                self.hovered_message = hit;
            }

            MouseEventKind::ScrollDown => {
                self.handle_scroll(mouse.column, mouse.row, 1)?;
            }
            MouseEventKind::ScrollUp => {
                self.handle_scroll(mouse.column, mouse.row, -1)?;
            }

            _ => {}
        }

        Ok(())
    }

    fn handle_scroll(&mut self, col: u16, row: u16, delta: i32) -> Result<()> {
        match self.current_view {
            View::Board => {
                if self.view_mode == ViewMode::SidePanel {
                    let hovered =
                        self.interaction_map
                            .resolve_message(col, row, InteractionKind::Hover);
                    match hovered {
                        Some(Message::SelectTaskInSidePanel(index)) => {
                            self.detail_focus = DetailFocus::List;
                            let rows = self.side_panel_rows();
                            if !rows.is_empty() {
                                let current = index.min(rows.len() - 1);
                                let next = if delta > 0 {
                                    (current + 1).min(rows.len() - 1)
                                } else {
                                    current.saturating_sub(1)
                                };
                                self.sync_side_panel_selection_at(&rows, next, true);
                            }
                            return Ok(());
                        }
                        Some(Message::FocusSidePanel(DetailFocus::List)) => {
                            self.detail_focus = DetailFocus::List;
                            let rows = self.side_panel_rows();
                            if !rows.is_empty() {
                                let current = self.side_panel_selected_row.min(rows.len() - 1);
                                let next = if delta > 0 {
                                    (current + 1).min(rows.len() - 1)
                                } else {
                                    current.saturating_sub(1)
                                };
                                self.sync_side_panel_selection_at(&rows, next, true);
                            }
                            return Ok(());
                        }
                        Some(Message::FocusSidePanel(DetailFocus::Details)) => {
                            self.detail_focus = DetailFocus::Details;
                            if delta > 0 {
                                self.scroll_details_down(1);
                            } else {
                                self.scroll_details_up(1);
                            }
                            return Ok(());
                        }
                        Some(Message::FocusSidePanel(DetailFocus::Log)) => {
                            self.detail_focus = DetailFocus::Log;
                            if delta > 0 {
                                self.scroll_log_down(1);
                            } else {
                                self.scroll_log_up(1);
                            }
                            return Ok(());
                        }
                        _ => {}
                    }

                    let rows = self.side_panel_rows();
                    match self.detail_focus {
                        DetailFocus::List => {
                            if !rows.is_empty() {
                                let current = self.side_panel_selected_row.min(rows.len() - 1);
                                let next = if delta > 0 {
                                    (current + 1).min(rows.len() - 1)
                                } else {
                                    current.saturating_sub(1)
                                };
                                self.sync_side_panel_selection_at(&rows, next, true);
                            }
                        }
                        DetailFocus::Details => {
                            if delta > 0 {
                                self.scroll_details_down(1);
                            } else {
                                self.scroll_details_up(1);
                            }
                        }
                        DetailFocus::Log => {
                            if delta > 0 {
                                self.scroll_log_down(1);
                            } else {
                                self.scroll_log_up(1);
                            }
                        }
                    }
                    return Ok(());
                }

                if let Some(Message::SelectTask(column, _) | Message::FocusColumn(column)) = self
                    .interaction_map
                    .resolve_message(col, row, InteractionKind::Hover)
                {
                    self.focused_column = column;
                }
                let max = self.max_scroll_offset_for_column(self.focused_column);
                let offset = self
                    .scroll_offset_per_column
                    .entry(self.focused_column)
                    .or_insert(0);
                if delta > 0 {
                    *offset = (*offset + 1).min(max);
                } else {
                    *offset = offset.saturating_sub(1);
                }
            }
            View::ProjectList => {
                if delta > 0 {
                    self.update(Message::ProjectListSelectDown)?;
                } else {
                    self.update(Message::ProjectListSelectUp)?;
                }
            }
            View::Archive => {
                if delta > 0 {
                    self.update(Message::ArchiveSelectDown)?;
                } else {
                    self.update(Message::ArchiveSelectUp)?;
                }
            }
            View::Settings => {
                if delta > 0 {
                    self.update(Message::SettingsNextItem)?;
                } else {
                    self.update(Message::SettingsPrevItem)?;
                }
            }
        }
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
        if self.current_view == View::Archive {
            return self.selected_archived_task();
        }

        match self.view_mode {
            ViewMode::Kanban => self.selected_task_in_column(self.focused_column),
            ViewMode::SidePanel => self.selected_task_in_side_panel(),
        }
    }

    fn selected_archived_task(&self) -> Option<Task> {
        self.archived_tasks
            .get(
                self.archive_selected_index
                    .min(self.archived_tasks.len().saturating_sub(1)),
            )
            .cloned()
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

    fn selected_task_in_side_panel(&self) -> Option<Task> {
        let rows = self.side_panel_rows();
        selected_task_from_side_panel_rows(&rows, self.side_panel_selected_row)
    }

    pub fn side_panel_rows(&self) -> Vec<SidePanelRow> {
        side_panel_rows_from(&self.categories, &self.tasks, &self.collapsed_categories)
    }

    fn cycle_detail_focus(&mut self) {
        let has_task = self.selected_task_in_side_panel().is_some();
        self.detail_focus = match (self.detail_focus, has_task) {
            (DetailFocus::List, _) => DetailFocus::Details,
            (DetailFocus::Details, true) => DetailFocus::Log,
            (DetailFocus::Details, false) => DetailFocus::List,
            (DetailFocus::Log, _) => DetailFocus::List,
        };
    }

    fn scroll_details_down(&mut self, step: usize) {
        self.detail_scroll_offset = self.detail_scroll_offset.saturating_add(step);
    }

    fn scroll_details_up(&mut self, step: usize) {
        self.detail_scroll_offset = self.detail_scroll_offset.saturating_sub(step);
    }

    fn log_entry_count(&self) -> usize {
        let Some(buffer) = self.current_log_buffer.as_deref() else {
            return 0;
        };

        let structured = buffer
            .lines()
            .filter(|line| line.starts_with("> ["))
            .count();
        if structured > 0 {
            return structured;
        }

        buffer
            .lines()
            .filter(|line| !line.trim().is_empty())
            .count()
    }

    fn scroll_log_down(&mut self, step: usize) {
        let max_offset = self.log_entry_count().saturating_sub(1);
        self.log_scroll_offset = self.log_scroll_offset.saturating_add(step).min(max_offset);
    }

    fn scroll_log_up(&mut self, step: usize) {
        self.log_scroll_offset = self.log_scroll_offset.saturating_sub(step);
    }

    fn scroll_expanded_log_down(&mut self, step: usize) {
        let max_offset = self.log_entry_count().saturating_sub(1);
        self.log_expanded_scroll_offset = self
            .log_expanded_scroll_offset
            .saturating_add(step)
            .min(max_offset);
    }

    fn scroll_expanded_log_up(&mut self, step: usize) {
        self.log_expanded_scroll_offset = self.log_expanded_scroll_offset.saturating_sub(step);
    }

    fn board_half_page_step(&self) -> usize {
        let content_lines = usize::from(self.viewport.1.saturating_sub(6));
        let visible_cards = (content_lines / 5).max(1);
        (visible_cards / 2).max(1)
    }

    fn side_panel_half_page_step(&self) -> usize {
        let content_lines = usize::from(self.viewport.1.saturating_sub(6));
        (content_lines / 4).max(1)
    }

    fn detail_half_page_step(&self) -> usize {
        let content_lines = usize::from(self.viewport.1.saturating_sub(8));
        (content_lines / 2).max(1)
    }

    fn move_selection_half_page_down(&mut self) {
        let step = self.board_half_page_step();
        if self.view_mode == ViewMode::SidePanel {
            match self.detail_focus {
                DetailFocus::List => {
                    let rows = self.side_panel_rows();
                    if rows.is_empty() {
                        self.side_panel_selected_row = 0;
                        self.current_log_buffer = None;
                    } else {
                        let current = self.side_panel_selected_row.min(rows.len() - 1);
                        let next = (current + self.side_panel_half_page_step()).min(rows.len() - 1);
                        self.sync_side_panel_selection_at(&rows, next, true);
                    }
                }
                DetailFocus::Details => self.scroll_details_down(self.detail_half_page_step()),
                DetailFocus::Log => self.scroll_log_down(self.detail_half_page_step()),
            }
        } else {
            let max_index = self.tasks_in_column(self.focused_column).saturating_sub(1);
            let selected = self
                .selected_task_per_column
                .entry(self.focused_column)
                .or_insert(0);
            *selected = selected.saturating_add(step).min(max_index);
        }
    }

    fn move_selection_half_page_up(&mut self) {
        let step = self.board_half_page_step();
        if self.view_mode == ViewMode::SidePanel {
            match self.detail_focus {
                DetailFocus::List => {
                    let rows = self.side_panel_rows();
                    if rows.is_empty() {
                        self.side_panel_selected_row = 0;
                        self.current_log_buffer = None;
                    } else {
                        let current = self.side_panel_selected_row.min(rows.len() - 1);
                        let prev = current.saturating_sub(self.side_panel_half_page_step());
                        self.sync_side_panel_selection_at(&rows, prev, true);
                    }
                }
                DetailFocus::Details => self.scroll_details_up(self.detail_half_page_step()),
                DetailFocus::Log => self.scroll_log_up(self.detail_half_page_step()),
            }
        } else if let Some(selected) = self.selected_task_per_column.get_mut(&self.focused_column) {
            *selected = selected.saturating_sub(step);
        }
    }

    fn move_selection_to_bottom(&mut self) {
        if self.view_mode == ViewMode::SidePanel {
            match self.detail_focus {
                DetailFocus::List => {
                    let rows = self.side_panel_rows();
                    if rows.is_empty() {
                        self.side_panel_selected_row = 0;
                        self.current_log_buffer = None;
                    } else {
                        self.sync_side_panel_selection_at(&rows, rows.len() - 1, true);
                    }
                }
                DetailFocus::Details => {
                    self.detail_scroll_offset = usize::MAX;
                }
                DetailFocus::Log => {
                    self.log_scroll_offset = self.log_entry_count().saturating_sub(1);
                }
            }
            return;
        }

        let max_index = self.tasks_in_column(self.focused_column).saturating_sub(1);
        let selected = self
            .selected_task_per_column
            .entry(self.focused_column)
            .or_insert(0);
        *selected = max_index;
    }

    fn move_selection_to_top(&mut self) {
        if self.view_mode == ViewMode::SidePanel {
            match self.detail_focus {
                DetailFocus::List => {
                    let rows = self.side_panel_rows();
                    if rows.is_empty() {
                        self.side_panel_selected_row = 0;
                        self.current_log_buffer = None;
                    } else {
                        self.sync_side_panel_selection_at(&rows, 0, true);
                    }
                }
                DetailFocus::Details => {
                    self.detail_scroll_offset = 0;
                }
                DetailFocus::Log => {
                    self.log_scroll_offset = 0;
                }
            }
            return;
        }

        let selected = self
            .selected_task_per_column
            .entry(self.focused_column)
            .or_insert(0);
        *selected = 0;
    }

    fn toggle_selected_log_entry(&mut self, use_expanded_offset: bool) {
        let entry_count = self.log_entry_count();
        if entry_count == 0 {
            return;
        }

        let selected = if use_expanded_offset {
            self.log_expanded_scroll_offset.min(entry_count - 1)
        } else {
            self.log_scroll_offset.min(entry_count - 1)
        };

        if !self.log_expanded_entries.insert(selected) {
            self.log_expanded_entries.remove(&selected);
        }
    }

    fn sync_side_panel_selection(&mut self, rows: &[SidePanelRow], clear_log: bool) {
        self.sync_side_panel_selection_at(rows, self.side_panel_selected_row, clear_log);
    }

    fn sync_side_panel_selection_at(
        &mut self,
        rows: &[SidePanelRow],
        index: usize,
        clear_log: bool,
    ) {
        if rows.is_empty() {
            self.side_panel_selected_row = 0;
            if clear_log {
                self.current_log_buffer = None;
                self.detail_scroll_offset = 0;
                self.log_scroll_offset = 0;
                self.log_expanded_scroll_offset = 0;
                self.log_expanded_entries.clear();
            }
            return;
        }

        let index = index.min(rows.len() - 1);
        self.side_panel_selected_row = index;

        match &rows[index] {
            SidePanelRow::CategoryHeader { column_index, .. } => {
                self.focused_column = (*column_index).min(self.categories.len().saturating_sub(1));
                self.selected_task_per_column
                    .entry(self.focused_column)
                    .or_insert(0);
            }
            SidePanelRow::Task {
                column_index,
                index_in_column,
                ..
            } => {
                self.focused_column = (*column_index).min(self.categories.len().saturating_sub(1));
                self.selected_task_per_column
                    .insert(*column_index, *index_in_column);
            }
        }

        if clear_log {
            self.current_log_buffer = None;
            self.detail_scroll_offset = 0;
            self.log_scroll_offset = 0;
            self.log_expanded_scroll_offset = 0;
            self.log_expanded_entries.clear();
        }
    }

    fn toggle_side_panel_category_collapse(&mut self) {
        let rows = self.side_panel_rows();
        if rows.is_empty() {
            self.side_panel_selected_row = 0;
            self.current_log_buffer = None;
            self.detail_scroll_offset = 0;
            self.log_scroll_offset = 0;
            self.log_expanded_scroll_offset = 0;
            self.log_expanded_entries.clear();
            return;
        }

        let selected = self.side_panel_selected_row.min(rows.len() - 1);
        let category_id = match &rows[selected] {
            SidePanelRow::CategoryHeader { category_id, .. } => *category_id,
            SidePanelRow::Task { .. } => return,
        };

        if !self.collapsed_categories.insert(category_id) {
            self.collapsed_categories.remove(&category_id);
        }

        let updated_rows = self.side_panel_rows();
        let next_index = updated_rows
            .iter()
            .position(|row| {
                matches!(
                    row,
                    SidePanelRow::CategoryHeader { category_id: id, .. } if *id == category_id
                )
            })
            .unwrap_or(0);
        self.sync_side_panel_selection_at(&updated_rows, next_index, true);
    }

    fn repo_for_task(&self, task: &Task) -> Option<Repo> {
        self.repos
            .iter()
            .find(|repo| repo.id == task.repo_id)
            .cloned()
    }

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
            remove_worktree: true,
            delete_branch: false,
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
        let ActiveDialog::DeleteTask(state) = self.active_dialog.clone() else {
            return Ok(());
        };

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

        self.db.update_task_status(task_id, Status::Idle.as_str())?;
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

fn create_task_error_dialog_state(err: &anyhow::Error) -> ErrorDialogState {
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

fn parse_existing_branch_name(detail: &str) -> Option<String> {
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

fn default_view_mode(settings: &crate::settings::Settings) -> ViewMode {
    if settings.default_view == "detail" {
        ViewMode::SidePanel
    } else {
        ViewMode::Kanban
    }
}

fn palette_index_for(current: Option<&str>) -> usize {
    CATEGORY_COLOR_PALETTE
        .iter()
        .position(|candidate| match (candidate, current) {
            (None, None) => true,
            (Some(expected), Some(actual)) => expected.eq_ignore_ascii_case(actual),
            _ => false,
        })
        .unwrap_or(0)
}

fn next_palette_color(current: Option<&str>) -> Option<String> {
    let next_idx = (palette_index_for(current) + 1) % CATEGORY_COLOR_PALETTE.len();
    CATEGORY_COLOR_PALETTE[next_idx].map(str::to_string)
}

impl Drop for App {
    fn drop(&mut self) {
        self.poller_stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.poller_thread.take() {
            handle.abort();
        }
    }
}

fn sorted_categories_with_indexes(categories: &[Category]) -> Vec<(usize, &Category)> {
    let mut out: Vec<(usize, &Category)> = categories.iter().enumerate().collect();
    out.sort_by_key(|(_, category)| category.position);
    out
}

fn side_panel_rows_from(
    categories: &[Category],
    tasks: &[Task],
    collapsed_categories: &HashSet<Uuid>,
) -> Vec<SidePanelRow> {
    let mut rows: Vec<SidePanelRow> = Vec::new();
    for (column_index, category) in sorted_categories_with_indexes(categories) {
        let mut category_tasks: Vec<Task> = tasks
            .iter()
            .filter(|task| task.category_id == category.id)
            .cloned()
            .collect();
        category_tasks.sort_by_key(|task| task.position);

        let collapsed = collapsed_categories.contains(&category.id);
        let total_tasks = category_tasks.len();
        let visible_tasks = if collapsed { 0 } else { total_tasks };

        rows.push(SidePanelRow::CategoryHeader {
            column_index,
            category_id: category.id,
            category_name: category.name.clone(),
            category_color: category.color.clone(),
            total_tasks,
            visible_tasks,
            collapsed,
        });

        if collapsed {
            continue;
        }

        for (index_in_column, task) in category_tasks.into_iter().enumerate() {
            rows.push(SidePanelRow::Task {
                column_index,
                index_in_column,
                category_id: category.id,
                task: Box::new(task),
            });
        }
    }
    rows
}

fn selected_task_from_side_panel_rows(rows: &[SidePanelRow], selected_row: usize) -> Option<Task> {
    if rows.is_empty() {
        return None;
    }
    let selected_row = selected_row.min(rows.len().saturating_sub(1));
    match rows.get(selected_row) {
        Some(SidePanelRow::Task { task, .. }) => Some(task.as_ref().clone()),
        _ => None,
    }
}

fn log_kind_label(raw: Option<&str>) -> String {
    let normalized = raw.unwrap_or("text").trim().to_ascii_lowercase();
    let value = match normalized.as_str() {
        "text" => "SAY".to_string(),
        "tool" => "TOOL".to_string(),
        "reasoning" => "THINK".to_string(),
        "step-start" => "STEP+".to_string(),
        "step-finish" => "STEP-".to_string(),
        "subtask" => "SUBTASK".to_string(),
        "patch" => "PATCH".to_string(),
        "agent" => "AGENT".to_string(),
        "snapshot" => "SNAP".to_string(),
        "retry" => "RETRY".to_string(),
        "compaction" => "COMPACT".to_string(),
        "file" => "FILE".to_string(),
        other => other.to_ascii_uppercase(),
    };

    if value.is_empty() {
        "TEXT".to_string()
    } else {
        value
    }
}

fn log_role_label(raw: Option<&str>) -> String {
    let value = raw
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_ascii_lowercase)
        .unwrap_or_else(|| "unknown".to_string());
    value.to_ascii_uppercase()
}

fn log_time_label(raw: Option<&str>) -> String {
    let Some(value) = raw.map(str::trim).filter(|value| !value.is_empty()) else {
        return "--:--:--".to_string();
    };

    if let Some(ts) = format_numeric_timestamp(value) {
        return ts;
    }

    if let Some((_, right)) = value.split_once('T') {
        let hhmmss = right.chars().take(8).collect::<String>();
        if hhmmss.len() == 8 {
            return hhmmss;
        }
    }

    if let Some((_, right)) = value.split_once(' ') {
        let hhmmss = right.chars().take(8).collect::<String>();
        if hhmmss.len() == 8 {
            return hhmmss;
        }
    }

    value.to_string()
}

fn format_numeric_timestamp(raw: &str) -> Option<String> {
    let value = raw.parse::<f64>().ok()?;
    if !value.is_finite() {
        return None;
    }

    let absolute = value.abs();
    let (seconds, nanos) = if absolute >= 1_000_000_000_000_000_000.0 {
        let sec = (value / 1_000_000_000.0).trunc() as i64;
        let nano = (value % 1_000_000_000.0).abs() as u32;
        (sec, nano)
    } else if absolute >= 1_000_000_000_000_000.0 {
        let sec = (value / 1_000_000.0).trunc() as i64;
        let nano = ((value % 1_000_000.0).abs() * 1_000.0) as u32;
        (sec, nano)
    } else if absolute >= 1_000_000_000_000.0 {
        let sec = (value / 1_000.0).trunc() as i64;
        let nano = ((value % 1_000.0).abs() * 1_000_000.0) as u32;
        (sec, nano)
    } else {
        let sec = value.trunc() as i64;
        let nano = ((value - value.trunc()).abs() * 1_000_000_000.0) as u32;
        (sec, nano)
    };

    let dt: DateTime<Utc> = DateTime::from_timestamp(seconds, nanos)?;
    Some(dt.with_timezone(&Local).format("%H:%M:%S").to_string())
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
        return Status::Idle.as_str().to_string();
    }

    if desired.expected_session_name.is_none() || !observed.session_exists {
        return Status::Idle.as_str().to_string();
    }

    observed
        .session_status
        .as_ref()
        .map(|status| status.state.as_str().to_string())
        .unwrap_or_else(|| {
            if SessionState::from_raw_status(current_status) == SessionState::Running {
                Status::Running.as_str().to_string()
            } else {
                Status::Idle.as_str().to_string()
            }
        })
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
        db.update_task_status(task.id, Status::Idle.as_str())?;
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

        if let Err(err) = db.increment_command_usage(&repo_selection_command_id(repo.id)) {
            warn!(
                error = %err,
                repo_id = %repo.id,
                "failed to persist repo selection usage"
            );
        }

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
        let path_exists = path.exists();
        if path_exists && runtime.git_is_valid_repo(&path) {
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

        let usage = repo_selection_usage_map(db);
        if let Some(repo_idx) = rank_repos_for_query(repo_path_input, repos, &usage)
            .first()
            .copied()
        {
            return Ok(repos[repo_idx].clone());
        }

        if path_exists {
            anyhow::bail!("not a git repository: {}", path.display());
        }

        anyhow::bail!("repo path does not exist: {}", path.display());
    }

    repos
        .get(state.repo_idx)
        .cloned()
        .context("select a repo or enter a repository path")
}

fn repo_selection_command_id(repo_id: Uuid) -> String {
    format!("{REPO_SELECTION_USAGE_PREFIX}{repo_id}")
}

fn repo_selection_usage_map(db: &Database) -> HashMap<Uuid, CommandFrequency> {
    db.get_command_frequencies()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|(command_id, frequency)| {
            let raw_repo_id = command_id.strip_prefix(REPO_SELECTION_USAGE_PREFIX)?;
            let repo_id = Uuid::parse_str(raw_repo_id).ok()?;
            Some((repo_id, frequency))
        })
        .collect()
}

fn rank_repos_for_query(
    query: &str,
    repos: &[Repo],
    usage: &HashMap<Uuid, CommandFrequency>,
) -> Vec<usize> {
    if repos.is_empty() {
        return Vec::new();
    }

    let now = Utc::now();
    let query = query.trim();
    let mut ranked: Vec<(usize, f64)> = Vec::with_capacity(repos.len());

    if query.is_empty() {
        for (repo_idx, repo) in repos.iter().enumerate() {
            ranked.push((repo_idx, repo_selection_bonus(repo.id, usage, now)));
        }
    } else {
        let mut matcher = Matcher::new(Config::DEFAULT);
        let mut query_buf = Vec::new();
        let query_utf32 = Utf32Str::new(query, &mut query_buf);
        let mut candidate_buf = Vec::new();
        let mut matched_indices = Vec::new();

        for (repo_idx, repo) in repos.iter().enumerate() {
            let mut best_match_score: Option<f64> = None;

            for (candidate, candidate_bonus) in repo_match_candidates(repo) {
                matched_indices.clear();
                let candidate_utf32 = Utf32Str::new(candidate.as_str(), &mut candidate_buf);
                if let Some(fuzzy_score) =
                    matcher.fuzzy_indices(candidate_utf32, query_utf32, &mut matched_indices)
                {
                    let score = f64::from(fuzzy_score) + candidate_bonus;
                    best_match_score = Some(match best_match_score {
                        Some(current) => current.max(score),
                        None => score,
                    });
                }
            }

            if let Some(best_match_score) = best_match_score {
                let score = best_match_score + repo_selection_bonus(repo.id, usage, now);
                ranked.push((repo_idx, score));
            }
        }
    }

    ranked.sort_by(|left, right| {
        right
            .1
            .partial_cmp(&left.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.0.cmp(&right.0))
    });

    ranked.into_iter().map(|(repo_idx, _)| repo_idx).collect()
}

fn repo_match_candidates(repo: &Repo) -> Vec<(String, f64)> {
    let mut out: Vec<(String, f64)> = Vec::new();
    let mut seen = HashSet::new();
    let mut add = |value: String, bonus: f64| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return;
        }
        let normalized = trimmed.to_ascii_lowercase();
        if seen.insert(normalized) {
            out.push((trimmed.to_string(), bonus));
        }
    };

    add(repo.name.clone(), 90.0);
    add(repo.path.clone(), 65.0);

    let path = Path::new(&repo.path);
    if let Some(file_name) = path.file_name().and_then(|value| value.to_str()) {
        add(file_name.to_string(), 85.0);
    }

    let segments: Vec<String> = path
        .components()
        .filter_map(|component| match component {
            std::path::Component::Normal(segment) => Some(segment.to_string_lossy().to_string()),
            _ => None,
        })
        .filter(|segment| !segment.is_empty())
        .collect();

    for segment in &segments {
        add(segment.to_string(), 80.0);
    }

    if segments.len() >= 2 {
        let suffix = format!(
            "{}/{}",
            segments[segments.len() - 2],
            segments[segments.len() - 1]
        );
        add(suffix, 88.0);
    }

    if segments.len() >= 3 {
        let suffix = format!(
            "{}/{}/{}",
            segments[segments.len() - 3],
            segments[segments.len() - 2],
            segments[segments.len() - 1]
        );
        add(suffix, 92.0);
    }

    out
}

fn repo_selection_bonus(
    repo_id: Uuid,
    usage: &HashMap<Uuid, CommandFrequency>,
    now: DateTime<Utc>,
) -> f64 {
    let Some(freq) = usage.get(&repo_id) else {
        return 0.0;
    };

    recency_frequency_bonus(
        freq.use_count,
        &freq.last_used,
        now,
        0.35,
        0.65,
        48.0,
        120.0,
    )
}

#[cfg(test)]
mod tests {
    use super::interaction::InteractionLayer;
    use super::*;

    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;
    use std::time::Instant;

    use crossterm::event::{
        KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
    };
    use tempfile::TempDir;
    use tuirealm::ratatui::widgets::ListState;

    use crate::keybindings::Keybindings;
    use crate::opencode::OpenCodeServerManager;

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
            "worktree creation failed: failed to create worktree `/home/cc/codes/playgrounds/.opencode-kanban-worktrees/test/c-2` for branch `c` from `main`: git command failed in /home/cc/codes/playgrounds/test: git worktree add -b c /home/cc/codes/playgrounds/.opencode-kanban-worktrees/test/c-2 main\nstdout:\nstderr: Preparing worktree (new branch 'c')\nfatal: a branch named 'c' already exists"
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
            detail_focus: DetailFocus::List,
            detail_scroll_offset: 0,
            log_scroll_offset: 0,
            log_split_ratio: 65,
            log_expanded: false,
            log_expanded_scroll_offset: 0,
            log_expanded_entries: HashSet::new(),
            session_todo_cache: Arc::new(Mutex::new(HashMap::new())),
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
        assert_eq!(confirmed_color, Some("cyan"));

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
        assert_eq!(canceled_color, Some("cyan"));

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
        assert_eq!(toggled_color, Some("cyan"));

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
