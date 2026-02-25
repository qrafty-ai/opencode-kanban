use std::time::Instant;

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::super::GG_SEQUENCE_TIMEOUT;
use super::super::dialogs;
use crate::app::{
    ActiveDialog, App, DetailFocus, Message, SettingsSection, TaskSearchMode, View, ViewMode,
};
use crate::keybindings::{KeyAction, KeyContext};

impl App {
    pub(crate) fn handle_key(&mut self, key: KeyEvent) -> Result<()> {
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

        if self.current_view == View::Board {
            if self.task_search.mode != TaskSearchMode::Inactive {
                match key.code {
                    KeyCode::Esc => {
                        self.update(Message::ExitTaskSearch)?;
                    }
                    KeyCode::Char('/') if key.modifiers == KeyModifiers::empty() => {
                        self.update(Message::StartTaskSearch)?;
                    }
                    KeyCode::Enter if self.task_search.mode == TaskSearchMode::Input => {
                        self.update(Message::ConfirmTaskSearch)?;
                    }
                    KeyCode::Backspace if self.task_search.mode == TaskSearchMode::Input => {
                        self.update(Message::TaskSearchBackspace)?;
                    }
                    KeyCode::Char(ch)
                        if self.task_search.mode == TaskSearchMode::Input
                            && !key.modifiers.contains(KeyModifiers::CONTROL)
                            && !key.modifiers.contains(KeyModifiers::ALT) =>
                    {
                        self.update(Message::TaskSearchAppend(ch))?;
                    }
                    KeyCode::Char('n') if self.task_search.mode == TaskSearchMode::Match => {
                        self.update(Message::TaskSearchNext)?;
                    }
                    KeyCode::Char('N') if self.task_search.mode == TaskSearchMode::Match => {
                        self.update(Message::TaskSearchPrev)?;
                    }
                    _ => {}
                }
                return Ok(());
            }

            if key.code == KeyCode::Char('/') && key.modifiers == KeyModifiers::empty() {
                self.update(Message::StartTaskSearch)?;
                return Ok(());
            }
        }

        if let Some(action) = self.keybindings.action_for_key(KeyContext::Global, key) {
            match action {
                KeyAction::ToggleHelp => self.active_dialog = ActiveDialog::Help,
                KeyAction::OpenPalette => {
                    self.update(Message::OpenCommandPalette)?;
                }
                KeyAction::Quit => self.should_quit = true,
                KeyAction::ToggleView => self.toggle_view_mode(),
                KeyAction::ShrinkPanel => {
                    self.side_panel_width = self.side_panel_width.saturating_sub(5).max(20);
                }
                KeyAction::ExpandPanel => {
                    self.side_panel_width = self.side_panel_width.saturating_add(5).min(80);
                }
                KeyAction::OpenArchiveView => {
                    self.update(Message::OpenArchiveView)?;
                }
                KeyAction::ProjectNext => {
                    if self.current_view == View::Board {
                        self.update(Message::SwitchToNextProject)?;
                    }
                }
                KeyAction::ProjectPrev => {
                    if self.current_view == View::Board {
                        self.update(Message::SwitchToPrevProject)?;
                    }
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
                    KeyAction::ProjectMoveUp => self.update(Message::ProjectListMoveUp)?,
                    KeyAction::ProjectMoveDown => self.update(Message::ProjectListMoveDown)?,
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
                KeyAction::EditTask => {
                    self.update(Message::OpenEditTaskDialog)?;
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
                KeyAction::OpenInNewTerminal => {
                    self.update(Message::OpenSelectedTaskInNewTerminal)?;
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

    pub(crate) fn handle_dialog_key(&mut self, key: KeyEvent) -> Result<()> {
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
}
