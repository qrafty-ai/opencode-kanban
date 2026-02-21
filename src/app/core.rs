use super::*;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct SubagentTodoSummary {
    pub title: String,
    pub todo_summary: Option<(usize, usize)>,
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
    pub(crate) started_at: Instant,
    pub(crate) mouse_seen: bool,
    pub(crate) mouse_hint_shown: bool,
    pub(crate) _server_manager: OpenCodeServerManager,
    pub(crate) poller_stop: Arc<AtomicBool>,
    pub(crate) poller_thread: Option<JoinHandle<()>>,
    pub view_mode: ViewMode,
    pub side_panel_width: u16,
    pub side_panel_selected_row: usize,
    pub archive_selected_index: usize,
    pub collapsed_categories: HashSet<Uuid>,
    pub current_log_buffer: Option<String>,
    pub current_change_summary: Option<GitChangeSummary>,
    pub detail_focus: DetailFocus,
    pub detail_scroll_offset: usize,
    pub log_scroll_offset: usize,
    pub log_split_ratio: u16,
    pub log_expanded: bool,
    pub log_expanded_scroll_offset: usize,
    pub log_expanded_entries: HashSet<usize>,
    pub session_todo_cache: Arc<Mutex<HashMap<Uuid, Vec<SessionTodoItem>>>>,
    pub session_subagent_cache: Arc<Mutex<HashMap<Uuid, Vec<SubagentTodoSummary>>>>,
    pub session_title_cache: Arc<Mutex<HashMap<String, String>>>,
    pub session_message_cache: Arc<Mutex<HashMap<Uuid, Vec<SessionMessageItem>>>>,
    pub todo_visualization_mode: TodoVisualizationMode,
    pub keybindings: Keybindings,
    pub settings: crate::settings::Settings,
    pub settings_view_state: Option<SettingsViewState>,
    pub category_edit_mode: bool,
    pub project_detail_cache: Option<ProjectDetailCache>,
    pub(crate) last_click: Option<(u16, u16, Instant)>,
    pub(crate) pending_gg_at: Option<Instant>,
}

pub(crate) fn load_project_detail(
    info: &crate::projects::ProjectInfo,
) -> Option<ProjectDetailCache> {
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
        let session_subagent_cache = Arc::new(Mutex::new(HashMap::new()));
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
            theme: Theme::resolve(effective_theme, &settings.custom_theme),
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
            current_change_summary: None,
            detail_focus: DetailFocus::List,
            detail_scroll_offset: 0,
            log_scroll_offset: 0,
            log_split_ratio: 65,
            log_expanded: false,
            log_expanded_scroll_offset: 0,
            log_expanded_entries: HashSet::new(),
            session_todo_cache,
            session_subagent_cache,
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
            Arc::clone(&app.session_subagent_cache),
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

    pub fn session_subagent_summaries(&self, task_id: Uuid) -> Vec<SubagentTodoSummary> {
        self.session_subagent_cache
            .lock()
            .ok()
            .and_then(|cache| cache.get(&task_id).cloned())
            .unwrap_or_default()
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

    pub(crate) fn build_log_buffer_from_messages(
        messages: &[SessionMessageItem],
    ) -> Option<String> {
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

    pub(crate) fn poller_db_path(&self) -> PathBuf {
        self.current_project_path
            .clone()
            .unwrap_or_else(|| projects::get_project_path(projects::DEFAULT_PROJECT))
    }

    pub(crate) fn restart_status_poller(&mut self) {
        self.poller_stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.poller_thread.take() {
            handle.abort();
        }

        self.poller_stop.store(false, Ordering::Relaxed);
        self.poller_thread = Some(polling::spawn_status_poller(
            self.poller_db_path(),
            Arc::clone(&self.poller_stop),
            Arc::clone(&self.session_todo_cache),
            Arc::clone(&self.session_subagent_cache),
            Arc::clone(&self.session_title_cache),
            Arc::clone(&self.session_message_cache),
            self.settings.poll_interval_ms,
        ));
    }

    pub(crate) fn save_settings_with_notice(&mut self) {
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
        if let Ok(mut cache) = self.session_subagent_cache.lock() {
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
        if let Ok(mut cache) = self.session_subagent_cache.lock() {
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
            Arc::clone(&self.session_subagent_cache),
            Arc::clone(&self.session_title_cache),
            Arc::clone(&self.session_message_cache),
            self.settings.poll_interval_ms,
        ));

        self.current_project_path = Some(path);
        Ok(())
    }

    pub(crate) fn current_project_slug_for_tmux(&self) -> Option<String> {
        let path = self.current_project_path.as_ref()?;
        let stem = path.file_stem()?.to_str()?;
        if stem == projects::DEFAULT_PROJECT {
            None
        } else {
            Some(stem.to_string())
        }
    }
}

impl Drop for App {
    fn drop(&mut self) {
        self.poller_stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.poller_thread.take() {
            handle.abort();
        }
    }
}
