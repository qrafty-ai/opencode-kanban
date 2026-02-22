use super::side_panel::{selected_task_from_side_panel_rows, side_panel_rows_from};
use super::{App, DetailFocus, SidePanelRow, TaskSearchMode, TaskSearchState, View, ViewMode};
use crate::types::{Repo, Task};
use uuid::Uuid;

impl App {
    pub(crate) fn toggle_view_mode(&mut self) {
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
                        rows.iter().position(
                            |row| matches!(row, SidePanelRow::Task { task, .. } if task.id == id),
                        )
                    })
                    .or_else(|| {
                        rows.iter()
                            .position(|row| matches!(row, SidePanelRow::CategoryHeader { .. }))
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

    fn task_location(&self, task_id: Uuid) -> Option<(usize, usize)> {
        self.categories
            .iter()
            .enumerate()
            .find_map(|(column_index, category)| {
                let mut tasks: Vec<&Task> = self
                    .tasks
                    .iter()
                    .filter(|task| task.category_id == category.id)
                    .collect();
                tasks.sort_by_key(|task| task.position);
                tasks
                    .iter()
                    .position(|task| task.id == task_id)
                    .map(|index_in_column| (column_index, index_in_column))
            })
    }

    pub(crate) fn focus_task_by_id(&mut self, task_id: Uuid) {
        let Some((column_index, index_in_column)) = self.task_location(task_id) else {
            return;
        };

        self.focused_column = column_index;
        self.selected_task_per_column
            .insert(column_index, index_in_column);

        if self.view_mode != ViewMode::SidePanel {
            return;
        }

        let rows = self.side_panel_rows();
        if rows.is_empty() {
            self.side_panel_selected_row = 0;
            return;
        }

        let row_index = rows
            .iter()
            .position(|row| matches!(row, SidePanelRow::Task { task, .. } if task.id == task_id))
            .or_else(|| {
                rows.iter().position(|row| {
                    matches!(
                        row,
                        SidePanelRow::CategoryHeader {
                            column_index: row_column,
                            ..
                        } if *row_column == column_index
                    )
                })
            });

        if let Some(row_index) = row_index {
            self.sync_side_panel_selection_at(&rows, row_index, false);
        }
    }

    pub(crate) fn tasks_in_column(&self, column_index: usize) -> usize {
        let Some(category) = self.categories.get(column_index) else {
            return 0;
        };
        self.tasks
            .iter()
            .filter(|task| task.category_id == category.id)
            .count()
    }

    pub(crate) fn max_scroll_offset_for_column(&self, column_index: usize) -> usize {
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

    pub(crate) fn selected_archived_task(&self) -> Option<Task> {
        self.archived_tasks
            .get(
                self.archive_selected_index
                    .min(self.archived_tasks.len().saturating_sub(1)),
            )
            .cloned()
    }

    pub(crate) fn selected_task_in_column(&self, column_index: usize) -> Option<Task> {
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

    pub(crate) fn selected_task_in_side_panel(&self) -> Option<Task> {
        let rows = self.side_panel_rows();
        selected_task_from_side_panel_rows(&rows, self.side_panel_selected_row)
    }

    pub fn side_panel_rows(&self) -> Vec<SidePanelRow> {
        side_panel_rows_from(&self.categories, &self.tasks, &self.collapsed_categories)
    }

    pub(crate) fn cycle_detail_focus(&mut self) {
        let has_task = self.selected_task_in_side_panel().is_some();
        self.detail_focus = match (self.detail_focus, has_task) {
            (DetailFocus::List, _) => DetailFocus::Details,
            (DetailFocus::Details, true) => DetailFocus::Log,
            (DetailFocus::Details, false) => DetailFocus::List,
            (DetailFocus::Log, _) => DetailFocus::List,
        };
    }

    pub(crate) fn scroll_details_down(&mut self, step: usize) {
        self.detail_scroll_offset = self.detail_scroll_offset.saturating_add(step);
    }

    pub(crate) fn scroll_details_up(&mut self, step: usize) {
        self.detail_scroll_offset = self.detail_scroll_offset.saturating_sub(step);
    }

    pub(crate) fn log_entry_count(&self) -> usize {
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

    pub(crate) fn scroll_log_down(&mut self, step: usize) {
        let max_offset = self.log_entry_count().saturating_sub(1);
        self.log_scroll_offset = self.log_scroll_offset.saturating_add(step).min(max_offset);
    }

    pub(crate) fn scroll_log_up(&mut self, step: usize) {
        self.log_scroll_offset = self.log_scroll_offset.saturating_sub(step);
    }

    pub(crate) fn scroll_expanded_log_down(&mut self, step: usize) {
        let max_offset = self.log_entry_count().saturating_sub(1);
        self.log_expanded_scroll_offset = self
            .log_expanded_scroll_offset
            .saturating_add(step)
            .min(max_offset);
    }

    pub(crate) fn scroll_expanded_log_up(&mut self, step: usize) {
        self.log_expanded_scroll_offset = self.log_expanded_scroll_offset.saturating_sub(step);
    }

    pub(crate) fn board_half_page_step(&self) -> usize {
        let content_lines = usize::from(self.viewport.1.saturating_sub(6));
        let visible_cards = (content_lines / 5).max(1);
        (visible_cards / 2).max(1)
    }

    pub(crate) fn side_panel_half_page_step(&self) -> usize {
        let content_lines = usize::from(self.viewport.1.saturating_sub(6));
        (content_lines / 4).max(1)
    }

    pub(crate) fn detail_half_page_step(&self) -> usize {
        let content_lines = usize::from(self.viewport.1.saturating_sub(8));
        (content_lines / 2).max(1)
    }

    pub(crate) fn move_selection_half_page_down(&mut self) {
        let step = self.board_half_page_step();
        if self.view_mode == ViewMode::SidePanel {
            match self.detail_focus {
                DetailFocus::List => {
                    let rows = self.side_panel_rows();
                    if rows.is_empty() {
                        self.side_panel_selected_row = 0;
                        self.current_log_buffer = None;
                        self.clear_current_change_summary();
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

    pub(crate) fn move_selection_half_page_up(&mut self) {
        let step = self.board_half_page_step();
        if self.view_mode == ViewMode::SidePanel {
            match self.detail_focus {
                DetailFocus::List => {
                    let rows = self.side_panel_rows();
                    if rows.is_empty() {
                        self.side_panel_selected_row = 0;
                        self.current_log_buffer = None;
                        self.clear_current_change_summary();
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

    pub(crate) fn move_selection_to_bottom(&mut self) {
        if self.view_mode == ViewMode::SidePanel {
            match self.detail_focus {
                DetailFocus::List => {
                    let rows = self.side_panel_rows();
                    if rows.is_empty() {
                        self.side_panel_selected_row = 0;
                        self.current_log_buffer = None;
                        self.clear_current_change_summary();
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

    pub(crate) fn move_selection_to_top(&mut self) {
        if self.view_mode == ViewMode::SidePanel {
            match self.detail_focus {
                DetailFocus::List => {
                    let rows = self.side_panel_rows();
                    if rows.is_empty() {
                        self.side_panel_selected_row = 0;
                        self.current_log_buffer = None;
                        self.clear_current_change_summary();
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

    pub(crate) fn toggle_selected_log_entry(&mut self, use_expanded_offset: bool) {
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

    pub(crate) fn sync_side_panel_selection(&mut self, rows: &[SidePanelRow], clear_log: bool) {
        self.sync_side_panel_selection_at(rows, self.side_panel_selected_row, clear_log);
    }

    pub(crate) fn sync_side_panel_selection_at(
        &mut self,
        rows: &[SidePanelRow],
        index: usize,
        clear_log: bool,
    ) {
        if rows.is_empty() {
            self.side_panel_selected_row = 0;
            if clear_log {
                self.current_log_buffer = None;
                self.clear_current_change_summary();
                self.detail_scroll_offset = 0;
                self.log_scroll_offset = 0;
                self.log_expanded_scroll_offset = 0;
                self.log_expanded_entries.clear();
            }
            return;
        }

        let index = index.min(rows.len() - 1);
        self.side_panel_selected_row = index;
        let mut selected_task: Option<Task> = None;

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
                task,
                ..
            } => {
                self.focused_column = (*column_index).min(self.categories.len().saturating_sub(1));
                self.selected_task_per_column
                    .insert(*column_index, *index_in_column);
                selected_task = Some((**task).clone());
            }
        }

        if clear_log {
            self.current_log_buffer = None;
            self.detail_scroll_offset = 0;
            self.log_scroll_offset = 0;
            self.log_expanded_scroll_offset = 0;
            self.log_expanded_entries.clear();
        }
        self.update_current_change_summary_for_task(selected_task.as_ref());
    }

    pub(crate) fn toggle_side_panel_category_collapse(&mut self) {
        let rows = self.side_panel_rows();
        if rows.is_empty() {
            self.side_panel_selected_row = 0;
            self.current_log_buffer = None;
            self.clear_current_change_summary();
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

    pub(crate) fn repo_for_task(&self, task: &Task) -> Option<Repo> {
        self.repos
            .iter()
            .find(|repo| repo.id == task.repo_id)
            .cloned()
    }

    pub(crate) fn start_task_search(&mut self) {
        self.task_search = TaskSearchState {
            mode: TaskSearchMode::Input,
            ..TaskSearchState::default()
        };
    }

    pub(crate) fn append_task_search_char(&mut self, ch: char) {
        self.task_search.query.push(ch);
    }

    pub(crate) fn pop_task_search_char(&mut self) {
        self.task_search.query.pop();
    }

    pub(crate) fn confirm_task_search(&mut self) {
        let query = self.task_search.query.trim().to_ascii_lowercase();
        let mut matches: Vec<&Task> = self
            .tasks
            .iter()
            .filter(|task| self.task_matches_search_query(task, &query))
            .collect();
        matches.sort_by_key(|task| {
            let category_position = self
                .categories
                .iter()
                .find(|category| category.id == task.category_id)
                .map(|category| category.position)
                .unwrap_or(i64::MAX);
            (category_position, task.position)
        });

        self.task_search.mode = TaskSearchMode::Match;
        self.task_search.matches = matches.iter().map(|task| task.id).collect();
        self.task_search.current_match_index = 0;
        self.focus_current_task_search_match();
    }

    fn task_matches_search_query(&self, task: &Task, query: &str) -> bool {
        let title_match = task.title.to_ascii_lowercase().contains(query);
        let branch_match = task.branch.to_ascii_lowercase().contains(query);
        let repo_match = self
            .repos
            .iter()
            .find(|repo| repo.id == task.repo_id)
            .map(|repo| repo.name.to_ascii_lowercase().contains(query))
            .unwrap_or(false);

        title_match || branch_match || repo_match
    }

    pub(crate) fn step_task_search_match(&mut self, delta: isize) {
        if self.task_search.matches.is_empty() {
            self.task_search.current_match_index = 0;
            return;
        }

        let len = self.task_search.matches.len() as isize;
        let next = (self.task_search.current_match_index as isize + delta).rem_euclid(len);
        self.task_search.current_match_index = next as usize;
        self.focus_current_task_search_match();
    }

    pub(crate) fn exit_task_search(&mut self) {
        self.task_search = TaskSearchState::default();
    }

    fn focus_current_task_search_match(&mut self) {
        let Some(task_id) = self
            .task_search
            .matches
            .get(self.task_search.current_match_index)
            .copied()
        else {
            return;
        };
        self.focus_task_by_id(task_id);
    }
}
