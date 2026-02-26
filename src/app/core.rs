use super::*;
use crate::task_palette::TaskPaletteCandidate;

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
    pub kanban_viewport_x: usize,
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
    pub current_change_summary_state: ChangeSummaryState,
    pub(crate) current_change_summary_key: Option<ChangeSummaryRequestKey>,
    pub(crate) change_summary_cache:
        HashMap<ChangeSummaryRequestKey, Result<GitChangeSummary, String>>,
    pub(crate) change_summary_in_flight: HashSet<ChangeSummaryRequestKey>,
    pub(crate) change_summary_generation: u64,
    pub(crate) change_summary_request_tx: Option<Sender<ChangeSummaryRequest>>,
    pub(crate) change_summary_result_rx: Receiver<ChangeSummaryResult>,
    pub(crate) pending_change_summary_results: Vec<ChangeSummaryResult>,
    pub(crate) change_summary_worker: Option<std::thread::JoinHandle<()>>,
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
    pub task_search: TaskSearchState,
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
        let (change_summary_request_tx, change_summary_result_rx, change_summary_worker) =
            spawn_change_summary_worker();
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
            kanban_viewport_x: 0,
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
            current_change_summary_state: ChangeSummaryState::Unavailable,
            current_change_summary_key: None,
            change_summary_cache: HashMap::new(),
            change_summary_in_flight: HashSet::new(),
            change_summary_generation: 0,
            change_summary_request_tx: Some(change_summary_request_tx),
            change_summary_result_rx,
            pending_change_summary_results: Vec::new(),
            change_summary_worker: Some(change_summary_worker),
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
            task_search: TaskSearchState::default(),
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
            app.settings.notification_display_duration_ms,
            app.current_project_slug_for_tmux(),
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
            self.settings.notification_display_duration_ms,
            self.current_project_slug_for_tmux(),
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

    pub(crate) fn sync_project_order_from_list(&mut self) {
        self.settings.project_order = self
            .project_list
            .iter()
            .map(|project| project.path.to_string_lossy().to_string())
            .collect();
    }

    pub(crate) fn persist_project_order_with_notice(&mut self) {
        self.sync_project_order_from_list();
        self.save_settings_with_notice();
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
        self.change_summary_cache
            .retain(|key, _| self.tasks.iter().any(|task| task.id == key.task_id));
        self.change_summary_in_flight
            .retain(|key| self.tasks.iter().any(|task| task.id == key.task_id));

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
            self.kanban_viewport_x = 0;
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

        if !self.settings.project_order.is_empty() {
            let rank_by_path = self
                .settings
                .project_order
                .iter()
                .enumerate()
                .map(|(idx, path)| (path.as_str(), idx))
                .collect::<std::collections::HashMap<_, _>>();

            self.project_list.sort_by(|left, right| {
                let left_key = left.path.to_string_lossy();
                let right_key = right.path.to_string_lossy();
                let left_rank = rank_by_path.get(left_key.as_ref()).copied();
                let right_rank = rank_by_path.get(right_key.as_ref()).copied();
                match (left_rank, right_rank) {
                    (Some(a), Some(b)) => a.cmp(&b).then_with(|| left.name.cmp(&right.name)),
                    (Some(_), None) => std::cmp::Ordering::Less,
                    (None, Some(_)) => std::cmp::Ordering::Greater,
                    (None, None) => left.name.cmp(&right.name),
                }
            });
        }

        self.sync_project_order_from_list();

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
        self.reset_change_summary_tracking();

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
        let project_slug = path.file_stem().and_then(|s| s.to_str()).and_then(|s| {
            if s == projects::DEFAULT_PROJECT {
                None
            } else {
                Some(s.to_string())
            }
        });
        self.poller_thread = Some(polling::spawn_status_poller(
            path.clone(),
            Arc::clone(&self.poller_stop),
            Arc::clone(&self.session_todo_cache),
            Arc::clone(&self.session_subagent_cache),
            Arc::clone(&self.session_title_cache),
            Arc::clone(&self.session_message_cache),
            self.settings.poll_interval_ms,
            self.settings.notification_display_duration_ms,
            project_slug,
        ));

        self.current_project_path = Some(path);
        Ok(())
    }

    pub(crate) fn task_palette_candidates(&self) -> Vec<TaskPaletteCandidate> {
        let mut candidates = Vec::new();

        for project in &self.project_list {
            if self.settings.is_archived_project_path(&project.path) {
                continue;
            }

            let Ok(db) = Database::open(&project.path) else {
                continue;
            };
            let Ok(mut tasks) = db.list_tasks() else {
                continue;
            };
            let Ok(repos) = db.list_repos() else {
                continue;
            };
            let Ok(categories) = db.list_categories() else {
                continue;
            };

            let repo_name_by_id: HashMap<Uuid, String> =
                repos.into_iter().map(|repo| (repo.id, repo.name)).collect();
            let category_name_by_id: HashMap<Uuid, String> = categories
                .iter()
                .map(|category| (category.id, category.name.clone()))
                .collect();
            let category_position_by_id: HashMap<Uuid, i64> = categories
                .into_iter()
                .map(|category| (category.id, category.position))
                .collect();

            tasks.sort_by_key(|task| {
                (
                    category_position_by_id
                        .get(&task.category_id)
                        .copied()
                        .unwrap_or(i64::MAX),
                    task.position,
                    task.created_at.clone(),
                )
            });

            for task in tasks {
                candidates.push(TaskPaletteCandidate {
                    project_name: project.name.clone(),
                    project_path: project.path.clone(),
                    task_id: task.id,
                    title: task.title,
                    branch: task.branch,
                    repo_name: repo_name_by_id
                        .get(&task.repo_id)
                        .cloned()
                        .unwrap_or_else(|| "unknown repo".to_string()),
                    category_name: category_name_by_id
                        .get(&task.category_id)
                        .cloned()
                        .unwrap_or_else(|| "unknown category".to_string()),
                });
            }
        }

        candidates
    }

    pub(crate) fn task_palette_scope_label(&self) -> String {
        let visible_project_count = self
            .project_list
            .iter()
            .filter(|project| !self.settings.is_archived_project_path(&project.path))
            .count();

        if visible_project_count == 1 {
            "Global (1 project)".to_string()
        } else {
            format!("Global ({visible_project_count} projects)")
        }
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

    pub(crate) fn clear_current_change_summary(&mut self) {
        self.current_change_summary = None;
        self.current_change_summary_state = ChangeSummaryState::Unavailable;
        self.current_change_summary_key = None;
    }

    pub(crate) fn task_change_summary_key(&self, task: &Task) -> Option<ChangeSummaryRequestKey> {
        let repo = self.repo_for_task(task)?;
        let source_path = task
            .worktree_path
            .as_deref()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(&repo.path));
        Some(ChangeSummaryRequestKey {
            task_id: task.id,
            source_path,
        })
    }

    pub(crate) fn queue_change_summary_request_if_needed(
        &mut self,
        key: &ChangeSummaryRequestKey,
    ) -> bool {
        if self.change_summary_in_flight.contains(key) {
            return true;
        }

        let Some(request_tx) = self.change_summary_request_tx.as_ref() else {
            return false;
        };

        let request = ChangeSummaryRequest {
            generation: self.change_summary_generation,
            key: key.clone(),
        };

        if request_tx.send(request).is_ok() {
            self.change_summary_in_flight.insert(key.clone());
            true
        } else {
            false
        }
    }

    pub(crate) fn apply_cached_change_summary(&mut self) {
        let Some(key) = self.current_change_summary_key.as_ref() else {
            return;
        };

        let Some(cached) = self.change_summary_cache.get(key) else {
            return;
        };

        match cached {
            Ok(summary) => {
                self.current_change_summary = Some(summary.clone());
                self.current_change_summary_state = ChangeSummaryState::Ready;
            }
            Err(err) => {
                self.current_change_summary = None;
                self.current_change_summary_state = ChangeSummaryState::Error(err.clone());
            }
        }
    }

    pub(crate) fn drain_change_summary_results(&mut self) {
        for result in self.pending_change_summary_results.drain(..) {
            if result.generation != self.change_summary_generation {
                continue;
            }
            self.change_summary_in_flight.remove(&result.key);
            self.change_summary_cache.insert(result.key, result.summary);
        }

        while let Ok(result) = self.change_summary_result_rx.try_recv() {
            if result.generation != self.change_summary_generation {
                continue;
            }
            self.change_summary_in_flight.remove(&result.key);
            self.change_summary_cache.insert(result.key, result.summary);
        }
        self.apply_cached_change_summary();
    }

    pub(crate) fn update_current_change_summary_for_task(&mut self, task: Option<&Task>) {
        let Some(task) = task else {
            self.clear_current_change_summary();
            return;
        };

        let Some(key) = self.task_change_summary_key(task) else {
            self.current_change_summary = None;
            self.current_change_summary_state = ChangeSummaryState::Unavailable;
            self.current_change_summary_key = None;
            return;
        };

        self.current_change_summary_key = Some(key.clone());

        if let Some(cached) = self.change_summary_cache.get(&key) {
            match cached {
                Ok(summary) => {
                    self.current_change_summary = Some(summary.clone());
                    self.current_change_summary_state = ChangeSummaryState::Ready;
                }
                Err(err) => {
                    self.current_change_summary = None;
                    self.current_change_summary_state = ChangeSummaryState::Error(err.clone());
                }
            }
            return;
        }

        self.current_change_summary = None;
        if self.queue_change_summary_request_if_needed(&key) {
            self.current_change_summary_state = ChangeSummaryState::Loading;
        } else {
            self.current_change_summary_state = ChangeSummaryState::Unavailable;
        }
    }

    pub(crate) fn collect_change_summary_result_for_port(&mut self) -> bool {
        match self.change_summary_result_rx.try_recv() {
            Ok(result) => {
                self.pending_change_summary_results.push(result);
                true
            }
            Err(std::sync::mpsc::TryRecvError::Empty)
            | Err(std::sync::mpsc::TryRecvError::Disconnected) => false,
        }
    }

    pub(crate) fn reset_change_summary_tracking(&mut self) {
        self.change_summary_generation = self.change_summary_generation.saturating_add(1);
        self.change_summary_cache.clear();
        self.change_summary_in_flight.clear();
        self.pending_change_summary_results.clear();
        self.clear_current_change_summary();
        while self.change_summary_result_rx.try_recv().is_ok() {}
    }
}

impl Drop for App {
    fn drop(&mut self) {
        self.poller_stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.poller_thread.take() {
            handle.abort();
        }
        self.change_summary_request_tx.take();
        if let Some(worker) = self.change_summary_worker.take() {
            let _ = worker.join();
        }
    }
}
