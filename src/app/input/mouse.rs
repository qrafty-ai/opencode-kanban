use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};

use crate::app::{
    App, ContextMenuItem, ContextMenuState, DetailFocus, InteractionKind, Message, Task, View,
    ViewMode,
};

impl App {
    pub(crate) fn handle_mouse(&mut self, mouse: MouseEvent) -> Result<()> {
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
                                    ContextMenuItem::Edit,
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

    pub(crate) fn handle_scroll(&mut self, col: u16, row: u16, delta: i32) -> Result<()> {
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
}
