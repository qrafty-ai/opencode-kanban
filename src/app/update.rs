use super::*;

impl App {
    pub fn update(&mut self, message: Message) -> Result<()> {
        match message {
            Message::Key(key) => self.handle_key(key)?,
            Message::Mouse(mouse) => self.handle_mouse(mouse)?,
            Message::Tick => {
                self.pulse_phase = (self.pulse_phase + 1) % 4;
                self.refresh_data()?;
                self.drain_change_summary_results();

                if self.view_mode == ViewMode::SidePanel {
                    let Some(task) = self.selected_task() else {
                        self.current_log_buffer = None;
                        self.clear_current_change_summary();
                        self.maybe_show_tmux_mouse_hint();
                        return Ok(());
                    };

                    if task.opencode_session_id.is_none() {
                        self.current_log_buffer = None;
                    } else {
                        let messages = self.session_messages(task.id);
                        self.current_log_buffer = Self::build_log_buffer_from_messages(&messages);
                    }

                    self.update_current_change_summary_for_task(Some(&task));
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
                    use_existing_directory: false,
                    existing_dir_input: String::new(),
                    branch_input: String::new(),
                    base_input: default_base,
                    title_input: String::new(),
                    ensure_base_up_to_date: true,
                    loading_message: None,
                    focused_field: NewTaskField::UseExistingDirectory,
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
                                state.general_selected_field.saturating_add(1).min(4);
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
                                    let current = ThemePreset::from_str(&self.settings.theme)
                                        .unwrap_or_default();
                                    let next = current.next();
                                    self.settings.theme = next.as_str().to_string();
                                    self.theme = Theme::resolve(next, &self.settings.custom_theme);
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
                                4 => {
                                    self.settings.board_alignment_mode =
                                        if self.settings.board_alignment_mode == "fit" {
                                            "scroll".to_string()
                                        } else {
                                            "fit".to_string()
                                        };
                                    if self.settings.board_alignment_mode == "fit" {
                                        self.kanban_viewport_x = 0;
                                    }
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
                            let current =
                                ThemePreset::from_str(&self.settings.theme).unwrap_or_default();
                            let previous = current.previous();
                            self.settings.theme = previous.as_str().to_string();
                            self.theme = Theme::resolve(previous, &self.settings.custom_theme);
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
                        4 => {
                            self.settings.board_alignment_mode =
                                if self.settings.board_alignment_mode == "fit" {
                                    "scroll".to_string()
                                } else {
                                    "fit".to_string()
                                };
                            if self.settings.board_alignment_mode == "fit" {
                                self.kanban_viewport_x = 0;
                            }
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
                            self.settings.theme = ThemePreset::Default.as_str().to_string();
                            self.theme =
                                Theme::resolve(ThemePreset::Default, &self.settings.custom_theme);
                        }
                        1 => {
                            self.settings.poll_interval_ms = 1_000;
                            self.restart_status_poller();
                        }
                        2 => {
                            self.settings.side_panel_width = 40;
                            self.side_panel_width = 40;
                        }
                        4 => {
                            self.settings.board_alignment_mode = "fit".to_string();
                            self.kanban_viewport_x = 0;
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
                    state.general_selected_field = index.min(4);
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
            Message::ToggleNewTaskExistingDirectory => {
                if let ActiveDialog::NewTask(state) = &mut self.active_dialog {
                    state.focused_field = NewTaskField::UseExistingDirectory;
                    state.use_existing_directory = !state.use_existing_directory;
                }
            }
            Message::SetNewTaskUseExistingDirectory(enabled) => {
                if let ActiveDialog::NewTask(state) = &mut self.active_dialog {
                    state.focused_field = NewTaskField::UseExistingDirectory;
                    state.use_existing_directory = enabled;
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
                    if field != DeleteTaskField::Delete {
                        state.confirm_destructive = false;
                    }
                }
            }
            Message::ToggleDeleteTaskCheckbox(field) => {
                if let ActiveDialog::DeleteTask(state) = &mut self.active_dialog {
                    state.focused_field = field;
                    state.confirm_destructive = false;
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
}
