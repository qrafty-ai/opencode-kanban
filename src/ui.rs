use std::path::Path;

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, Clear, Paragraph, Scrollbar, ScrollbarOrientation,
        ScrollbarState, Wrap,
    },
};

use crate::app::{
    ActiveDialog, App, CategoryInputField, CategoryInputMode, DeleteCategoryField, DeleteTaskField,
    Message, NewTaskField, WorktreeNotFoundField,
};
use crate::command_palette::{CommandPaletteState, all_commands};
use crate::theme::{Theme, parse_color};
use crate::types::Task;

#[derive(Clone, Copy)]
pub enum OverlayAnchor {
    Center,
    Top,
}

pub struct OverlayConfig<'a> {
    pub title: &'a str,
    pub anchor: OverlayAnchor,
    pub width_percent: u16,
    pub height_percent: u16,
}

fn calculate_overlay_area(
    anchor: OverlayAnchor,
    width_percent: u16,
    height_percent: u16,
    area: Rect,
) -> Rect {
    match anchor {
        OverlayAnchor::Center => centered_rect(width_percent, height_percent, area),
        OverlayAnchor::Top => {
            let popup_layout = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(2),
                    Constraint::Percentage(height_percent),
                    Constraint::Min(0),
                ])
                .split(area);

            Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage((100 - width_percent) / 2),
                    Constraint::Percentage(width_percent),
                    Constraint::Percentage((100 - width_percent) / 2),
                ])
                .split(popup_layout[1])[1]
        }
    }
}

fn render_overlay_frame(frame: &mut Frame<'_>, config: OverlayConfig) -> Rect {
    let theme = Theme::default();
    let area = calculate_overlay_area(
        config.anchor,
        config.width_percent,
        config.height_percent,
        frame.area(),
    );

    frame.render_widget(Clear, area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(theme.focus))
        .title(Span::styled(config.title, Style::default().fg(theme.focus)))
        .title_alignment(Alignment::Center)
        .style(Style::default().bg(Color::Black));

    let inner_area = block.inner(area);
    frame.render_widget(block, area);

    inner_area
}

pub fn render(frame: &mut Frame<'_>, app: &mut App) {
    app.hit_test_map.clear();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(frame.area());

    render_header(frame, chunks[0], app);
    render_columns(frame, chunks[1], app);
    render_footer(frame, chunks[2], app);

    if app.active_dialog != ActiveDialog::None {
        render_dialog(frame, app);
    }
}

fn render_header(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let theme = Theme::default();
    let header = Block::default()
        .borders(Borders::TOP | Borders::LEFT | Borders::RIGHT)
        .border_style(Style::default().fg(theme.header))
        .title(Span::styled(
            " opencode-kanban ",
            Style::default().fg(theme.header),
        ))
        .title_alignment(Alignment::Left);

    let refresh_info = format!(" {} tasks - auto-refresh: 3s ", app.tasks.len());
    let header_right = Block::default()
        .title(Span::styled(
            refresh_info,
            Style::default().fg(theme.header),
        ))
        .title_alignment(Alignment::Right);

    frame.render_widget(header, area);
    frame.render_widget(header_right, area);
}

fn render_footer(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let theme = Theme::default();
    let notice = app.footer_notice.as_deref().unwrap_or(
        " n: new task  Enter: attach  Ctrl+P: command palette  c/r/x: category  H/L: move task left/right  J/K: reorder task  tmux Prefix+K: previous session ",
    );
    let footer = Block::default()
        .borders(Borders::BOTTOM | Borders::LEFT | Borders::RIGHT)
        .border_style(Style::default().fg(theme.header))
        .title(Span::styled(
            format!(" {notice} "),
            Style::default().fg(theme.header),
        ))
        .title_alignment(Alignment::Center);
    frame.render_widget(footer, area);
}

fn render_columns(frame: &mut Frame<'_>, area: Rect, app: &mut App) {
    let theme = Theme::default();
    if app.categories.is_empty() {
        render_empty_state(frame, area);
        return;
    }

    let min_column_width = 18;
    if area.width < (app.categories.len() as u16).saturating_mul(min_column_width) {
        let msg = Paragraph::new(format!(
            "Terminal too narrow for {} columns. Increase width to at least {} cells.",
            app.categories.len(),
            (app.categories.len() as u16).saturating_mul(min_column_width)
        ))
        .alignment(Alignment::Center)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Double)
                .border_style(Style::default().fg(theme.secondary))
                .title(Span::styled(
                    " Resize Needed ",
                    Style::default().fg(theme.secondary),
                )),
        );
        frame.render_widget(msg, area);
        return;
    }

    let constraints: Vec<Constraint> = (0..app.categories.len())
        .map(|_| Constraint::Ratio(1, app.categories.len() as u32))
        .collect();

    let column_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(constraints)
        .split(area);

    for (i, category) in app.categories.iter().enumerate() {
        let is_focused = i == app.focused_column;
        let border_type = if is_focused {
            BorderType::Double
        } else {
            BorderType::Plain
        };

        let category_color = category
            .color
            .as_deref()
            .and_then(parse_color)
            .unwrap_or(theme.column);
        let border_color = if is_focused {
            theme.focus
        } else {
            category_color
        };

        let tasks_in_col: Vec<&Task> = app
            .tasks
            .iter()
            .filter(|t| t.category_id == category.id)
            .collect();
        let mut tasks_sorted = tasks_in_col.clone();
        tasks_sorted.sort_by_key(|t| t.position);
        let scroll_offset = app.clamped_scroll_offset_for_column(i);

        let title = format!(" {} ({}) ", category.name, tasks_sorted.len());
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(border_type)
            .border_style(Style::default().fg(border_color))
            .title(Span::styled(title, Style::default().fg(border_color)))
            .title_alignment(Alignment::Center);

        let inner_area = block.inner(column_chunks[i]);
        frame.render_widget(block, column_chunks[i]);

        app.hit_test_map.push((
            Rect {
                x: column_chunks[i].x,
                y: column_chunks[i].y,
                width: column_chunks[i].width,
                height: 1,
            },
            Message::FocusColumn(i),
        ));

        let mut y_offset = 0;
        for (j, task) in tasks_sorted.iter().enumerate().skip(scroll_offset) {
            if y_offset + 2 > inner_area.height {
                break;
            }

            let is_selected = is_focused && app.selected_task_per_column.get(&i) == Some(&j);

            let status_icon = match task.tmux_status.as_str() {
                "running" => "●",
                "idle" => "○",
                "waiting" => "◐",
                "dead" => "✕",
                "repo_unavailable" => "!",
                "broken" => "!",
                _ => "?",
            };

            let status_color = match task.tmux_status.as_str() {
                "running" => theme.focus,
                "idle" => theme.task,
                "waiting" => theme.secondary,
                "dead" => theme.secondary,
                "repo_unavailable" => theme.secondary,
                "broken" => theme.secondary,
                _ => theme.secondary,
            };

            let prefix = if is_selected { "▸" } else { " " };
            let bg_color = if is_selected {
                theme.secondary
            } else {
                Color::Reset
            };

            let line1 = Line::from(vec![
                Span::styled(prefix, Style::default().fg(theme.focus)),
                Span::styled(status_icon, Style::default().fg(status_color)),
                Span::raw(" "),
                Span::styled(&task.title, Style::default().fg(theme.task)),
            ]);

            let repo = app.repos.iter().find(|repo| repo.id == task.repo_id);
            let repo_name = repo.map(|repo| repo.name.as_str()).unwrap_or("unknown");
            let repo_available = repo
                .map(|repo| Path::new(&repo.path).exists())
                .unwrap_or(false);
            let repo_label = if repo_available {
                format!("{}:{}", repo_name, task.branch)
            } else {
                format!("{}:{} (repo unavailable)", repo_name, task.branch)
            };

            let line2 = Line::from(vec![
                Span::raw("   "),
                Span::styled(repo_label, Style::default().fg(theme.secondary)),
            ]);

            let task_area = Rect {
                x: inner_area.x,
                y: inner_area.y + y_offset,
                width: inner_area.width,
                height: 2,
            };

            let paragraph = Paragraph::new(vec![line1, line2]).style(Style::default().bg(bg_color));

            frame.render_widget(paragraph, task_area);

            app.hit_test_map
                .push((task_area, Message::SelectTask(i, j)));

            y_offset += 3;
        }

        if tasks_sorted.is_empty() {
            frame.render_widget(
                Paragraph::new("No tasks in this category")
                    .alignment(Alignment::Center)
                    .style(Style::default().fg(theme.secondary)),
                inner_area,
            );
        } else {
            // Render scrollbar if there are more tasks than visible
            let total_tasks = tasks_sorted.len() as u16;
            let visible_tasks = inner_area.height / 3;
            if total_tasks > visible_tasks {
                let mut scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
                    .thumb_style(Style::default().fg(category_color).bg(theme.secondary));
                scrollbar = scrollbar
                    .track_symbol(Some("│"))
                    .begin_symbol(Some("↑"))
                    .end_symbol(Some("↓"));
                frame.render_stateful_widget(
                    scrollbar,
                    Rect {
                        x: column_chunks[i].x + column_chunks[i].width - 1,
                        y: inner_area.y,
                        height: inner_area.height,
                        width: 1,
                    },
                    &mut app.column_scroll_states[i],
                );
            }
        }
    }
}

fn render_empty_state(frame: &mut Frame<'_>, area: Rect) {
    let theme = Theme::default();
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.header))
        .title(Span::styled(" Welcome ", Style::default().fg(theme.header)))
        .title_alignment(Alignment::Center);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    frame.render_widget(
        Paragraph::new(
            "No tasks yet. Press n to create your first task.\nPress ? to view keybindings and mouse actions.",
        )
        .alignment(Alignment::Center)
        .style(Style::default().fg(theme.task)),
        inner,
    );
}

fn render_command_palette(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &CommandPaletteState,
    show_results: bool,
) {
    let theme = Theme::default();
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(area);

    let search_block = Block::default()
        .borders(Borders::ALL)
        .title(" Search ")
        .border_style(Style::default().fg(theme.focus));

    let search_text = Paragraph::new(state.query.as_str()).block(search_block);
    frame.render_widget(search_text, layout[0]);

    if !show_results {
        return;
    }

    if state.filtered.is_empty() {
        let no_match = Paragraph::new("No matching commands")
            .style(Style::default().fg(theme.secondary))
            .alignment(Alignment::Center);
        frame.render_widget(no_match, layout[1]);
        return;
    }

    let all_cmds = all_commands();
    let list_height = layout[1].height as usize;
    let total_items = state.filtered.len();

    let scroll_offset = if total_items <= list_height {
        0
    } else {
        let half_height = list_height / 2;
        if state.selected_index > half_height {
            (state.selected_index - half_height).min(total_items - list_height)
        } else {
            0
        }
    };

    let visible_commands = state.filtered.iter().skip(scroll_offset).take(list_height);

    for (i, ranked_cmd) in visible_commands.enumerate() {
        let cmd_def = &all_cmds[ranked_cmd.command_idx];
        let is_selected = (scroll_offset + i) == state.selected_index;

        let (prefix, bg_style) = if is_selected {
            ("▸ ", Style::default().bg(theme.secondary))
        } else {
            ("  ", Style::default())
        };

        let mut spans = vec![Span::styled(prefix, Style::default().fg(theme.focus))];

        let name = cmd_def.display_name;
        let mut last_idx = 0;

        for &idx in &ranked_cmd.matched_indices {
            if idx >= name.len() {
                continue;
            }

            if idx > last_idx {
                spans.push(Span::raw(&name[last_idx..idx]));
            }
            spans.push(Span::styled(
                &name[idx..idx + 1],
                Style::default()
                    .fg(theme.focus)
                    .add_modifier(ratatui::style::Modifier::BOLD),
            ));
            last_idx = idx + 1;
        }
        if last_idx < name.len() {
            spans.push(Span::raw(&name[last_idx..]));
        }

        let row_area = Rect {
            x: layout[1].x,
            y: layout[1].y + i as u16,
            width: layout[1].width,
            height: 1,
        };

        frame.render_widget(Paragraph::new(Line::from(spans)).style(bg_style), row_area);

        if !cmd_def.keybinding.is_empty() {
            let key_hint = Span::styled(cmd_def.keybinding, Style::default().fg(theme.secondary));
            frame.render_widget(
                Paragraph::new(Line::from(key_hint)).alignment(Alignment::Right),
                row_area,
            );
        }
    }

    if total_items > list_height {
        let mut scrollbar_state =
            ScrollbarState::new(total_items.saturating_sub(1)).position(scroll_offset);
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .thumb_style(Style::default().fg(theme.secondary).bg(theme.secondary))
            .track_symbol(Some("│"))
            .begin_symbol(Some("↑"))
            .end_symbol(Some("↓"));

        frame.render_stateful_widget(scrollbar, layout[1], &mut scrollbar_state);
    }
}

fn render_dialog(frame: &mut Frame<'_>, app: &mut App) {
    if matches!(app.active_dialog, ActiveDialog::Help) {
        render_help_overlay(frame);
        return;
    }

    let (percent_x, percent_y) = match &app.active_dialog {
        ActiveDialog::NewTask(_) => (80, 72),
        ActiveDialog::DeleteTask(_) => (50, 50),
        ActiveDialog::CategoryInput(_) => (50, 50),
        ActiveDialog::DeleteCategory(_) => (50, 50),
        ActiveDialog::WorktreeNotFound(_) => (60, 50),
        ActiveDialog::RepoUnavailable(_) => (60, 50),
        ActiveDialog::Error(_) => (60, 60),
        ActiveDialog::CommandPalette(_) => command_palette_overlay_size(app.viewport),
        _ => (60, 20),
    };

    let title = match &app.active_dialog {
        ActiveDialog::NewTask(_) => " New Task ",
        ActiveDialog::CategoryInput(state) => match state.mode {
            CategoryInputMode::Add => " Add Category ",
            CategoryInputMode::Rename => " Rename Category ",
        },
        ActiveDialog::DeleteCategory(_) => " Delete Category ",
        ActiveDialog::Error(_) => " Error ",
        ActiveDialog::DeleteTask(_) => " Delete Task ",
        ActiveDialog::CommandPalette(_) => " Command Palette ",
        ActiveDialog::MoveTask(_) => " Move Task ",
        ActiveDialog::WorktreeNotFound(_) => " Worktree Not Found ",
        ActiveDialog::RepoUnavailable(_) => " Repo Unavailable ",
        ActiveDialog::Help => " Help ",
        ActiveDialog::None => "",
    };

    let anchor = match &app.active_dialog {
        ActiveDialog::CommandPalette(_) => OverlayAnchor::Top,
        _ => OverlayAnchor::Center,
    };

    let inner_area = render_overlay_frame(
        frame,
        OverlayConfig {
            title,
            anchor,
            width_percent: percent_x,
            height_percent: percent_y,
        },
    );

    match &app.active_dialog {
        ActiveDialog::CommandPalette(state) => {
            render_command_palette(
                frame,
                inner_area,
                state,
                should_render_command_palette_results(app.viewport),
            );
        }
        ActiveDialog::NewTask(state) => {
            let layout = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(3),
                    Constraint::Length(3),
                    Constraint::Length(3),
                    Constraint::Length(3),
                    Constraint::Length(1),
                    Constraint::Length(3),
                ])
                .split(inner_area);

            let repo_name = if !app.repos.is_empty() {
                if state.repo_input.trim().is_empty() {
                    &app.repos[state.repo_idx].name
                } else {
                    &state.repo_input
                }
            } else if state.repo_input.trim().is_empty() {
                "No repos found"
            } else {
                &state.repo_input
            };
            render_input_field(
                frame,
                layout[0],
                "Repo (name or path)",
                repo_name,
                state.focused_field == NewTaskField::Repo,
            );

            render_input_field(
                frame,
                layout[1],
                "Branch",
                &state.branch_input,
                state.focused_field == NewTaskField::Branch,
            );

            render_input_field(
                frame,
                layout[2],
                "Base",
                &state.base_input,
                state.focused_field == NewTaskField::Base,
            );

            render_input_field(
                frame,
                layout[3],
                "Title",
                &state.title_input,
                state.focused_field == NewTaskField::Title,
            );

            let checkbox_text = if state.ensure_base_up_to_date {
                "[x] Ensure base branch up-to-date"
            } else {
                "[ ] Ensure base branch up-to-date"
            };
            let checkbox_block = Block::default()
                .borders(Borders::NONE)
                .style(Style::default().fg(Color::White));
            let checkbox_paragraph = Paragraph::new(checkbox_text)
                .block(checkbox_block)
                .alignment(Alignment::Center);
            let checkbox_focused = state.focused_field == NewTaskField::EnsureBaseUpToDate;
            if checkbox_focused {
                frame.render_widget(
                    checkbox_paragraph
                        .style(Style::default().fg(Color::Yellow).bg(Color::DarkGray)),
                    layout[4],
                );
            } else {
                frame.render_widget(checkbox_paragraph, layout[4]);
            }

            let button_layout = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(layout[5]);

            render_button(
                frame,
                button_layout[0],
                if state.loading_message.is_some() {
                    "[ Creating... ]"
                } else {
                    "[ Create ]"
                },
                state.focused_field == NewTaskField::Create,
            );
            render_button(
                frame,
                button_layout[1],
                "[ Cancel ]",
                state.focused_field == NewTaskField::Cancel,
            );

            app.hit_test_map
                .push((button_layout[0], Message::CreateTask));
            app.hit_test_map
                .push((button_layout[1], Message::DismissDialog));

            if let Some(loading_message) = state.loading_message.as_deref() {
                let loading_area = Rect {
                    x: inner_area.x,
                    y: inner_area.y + inner_area.height.saturating_sub(1),
                    width: inner_area.width,
                    height: 1,
                };
                frame.render_widget(
                    Paragraph::new(loading_message).alignment(Alignment::Center),
                    loading_area,
                );
            }
        }
        ActiveDialog::DeleteTask(state) => {
            let layout = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(2),
                    Constraint::Length(1),
                    Constraint::Length(1),
                    Constraint::Length(1),
                    Constraint::Length(3),
                ])
                .split(inner_area);

            let text = format!(
                "Delete \"{}\"?\n({}:{})",
                state.task_title, "repo", state.task_branch
            );
            frame.render_widget(Paragraph::new(text).alignment(Alignment::Center), layout[0]);

            render_checkbox(
                frame,
                layout[1],
                "Kill tmux session",
                state.kill_tmux,
                state.focused_field == DeleteTaskField::KillTmux,
            );
            render_checkbox(
                frame,
                layout[2],
                "Remove worktree",
                state.remove_worktree,
                state.focused_field == DeleteTaskField::RemoveWorktree,
            );
            render_checkbox(
                frame,
                layout[3],
                "Delete branch",
                state.delete_branch,
                state.focused_field == DeleteTaskField::DeleteBranch,
            );

            let button_layout = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(layout[4]);

            render_button(
                frame,
                button_layout[0],
                "[ Delete ]",
                state.focused_field == DeleteTaskField::Delete,
            );
            render_button(
                frame,
                button_layout[1],
                "[ Cancel ]",
                state.focused_field == DeleteTaskField::Cancel,
            );

            app.hit_test_map
                .push((layout[1], Message::DeleteTaskToggleKillTmux));
            app.hit_test_map
                .push((layout[2], Message::DeleteTaskToggleRemoveWorktree));
            app.hit_test_map
                .push((layout[3], Message::DeleteTaskToggleDeleteBranch));
            app.hit_test_map
                .push((button_layout[0], Message::ConfirmDeleteTask));
            app.hit_test_map
                .push((button_layout[1], Message::DismissDialog));
        }
        ActiveDialog::CategoryInput(state) => {
            let layout = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(3),
                    Constraint::Length(1),
                    Constraint::Length(3),
                ])
                .split(inner_area);

            render_input_field(
                frame,
                layout[0],
                "Name",
                &state.name_input,
                state.focused_field == CategoryInputField::Name,
            );

            let buttons = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(layout[2]);

            render_button(
                frame,
                buttons[0],
                if state.mode == CategoryInputMode::Add {
                    "[ Add ]"
                } else {
                    "[ Rename ]"
                },
                state.focused_field == CategoryInputField::Confirm,
            );
            render_button(
                frame,
                buttons[1],
                "[ Cancel ]",
                state.focused_field == CategoryInputField::Cancel,
            );

            app.hit_test_map
                .push((buttons[0], Message::SubmitCategoryInput));
            app.hit_test_map.push((buttons[1], Message::DismissDialog));
        }
        ActiveDialog::DeleteCategory(state) => {
            let layout = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(4), Constraint::Length(3)])
                .split(inner_area);

            let detail = if state.task_count == 0 {
                format!("Delete category '{}' ?", state.category_name)
            } else {
                format!(
                    "'{}' still has {} task(s).\nDeletion will fail.",
                    state.category_name, state.task_count
                )
            };
            frame.render_widget(
                Paragraph::new(detail).alignment(Alignment::Center),
                layout[0],
            );

            let buttons = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(layout[1]);

            render_button(
                frame,
                buttons[0],
                "[ Delete ]",
                state.focused_field == DeleteCategoryField::Delete,
            );
            render_button(
                frame,
                buttons[1],
                "[ Cancel ]",
                state.focused_field == DeleteCategoryField::Cancel,
            );

            app.hit_test_map
                .push((buttons[0], Message::ConfirmDeleteCategory));
            app.hit_test_map.push((buttons[1], Message::DismissDialog));
        }
        ActiveDialog::Help => {}
        ActiveDialog::WorktreeNotFound(state) => {
            let layout = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Min(5),
                    Constraint::Length(1),
                    Constraint::Length(3),
                ])
                .split(inner_area);

            let text = format!(
                "Worktree not found for \"{}\".\n\nRecreate?",
                state.task_title
            );
            let paragraph = Paragraph::new(text)
                .alignment(Alignment::Center)
                .wrap(Wrap { trim: true });
            frame.render_widget(paragraph, layout[0]);

            let buttons = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(34),
                    Constraint::Percentage(33),
                    Constraint::Percentage(33),
                ])
                .split(layout[2]);

            render_button(
                frame,
                buttons[0],
                "[ Recreate ]",
                state.focused_field == WorktreeNotFoundField::Recreate,
            );
            render_button(
                frame,
                buttons[1],
                "[ Mark as broken ]",
                state.focused_field == WorktreeNotFoundField::MarkBroken,
            );
            render_button(
                frame,
                buttons[2],
                "[ Cancel ]",
                state.focused_field == WorktreeNotFoundField::Cancel,
            );

            app.hit_test_map
                .push((buttons[0], Message::WorktreeNotFoundRecreate));
            app.hit_test_map
                .push((buttons[1], Message::WorktreeNotFoundMarkBroken));
            app.hit_test_map.push((buttons[2], Message::DismissDialog));
        }
        ActiveDialog::RepoUnavailable(state) => {
            let layout = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(6), Constraint::Length(3)])
                .split(inner_area);

            let text = format!(
                "Repo unavailable for \"{}\"\n\n{}",
                state.task_title, state.repo_path
            );
            let paragraph = Paragraph::new(text)
                .alignment(Alignment::Center)
                .wrap(Wrap { trim: true });
            frame.render_widget(paragraph, layout[0]);

            render_button(frame, layout[1], "[ Dismiss ]", true);
            app.hit_test_map
                .push((layout[1], Message::RepoUnavailableDismiss));
        }
        ActiveDialog::Error(state) => {
            let layout = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(8), Constraint::Length(3)])
                .split(inner_area);

            let text = format!("{}\n\n{}", state.title, state.detail);
            let paragraph = Paragraph::new(text)
                .alignment(Alignment::Center)
                .wrap(Wrap { trim: true });
            frame.render_widget(paragraph, layout[0]);
            render_button(frame, layout[1], "[ Dismiss ]", true);
            app.hit_test_map.push((layout[1], Message::DismissDialog));
        }
        _ => {}
    }
}

fn command_palette_overlay_size(viewport: (u16, u16)) -> (u16, u16) {
    let width = if viewport.0 < 30 { 90 } else { 60 };
    (width, 50)
}

fn should_render_command_palette_results(viewport: (u16, u16)) -> bool {
    viewport.1 >= 10
}

fn render_input_field(
    frame: &mut Frame<'_>,
    area: Rect,
    label: &str,
    value: &str,
    is_focused: bool,
) {
    let theme = Theme::default();
    let block = Block::default()
        .borders(Borders::ALL)
        .title(label)
        .style(if is_focused {
            Style::default().fg(theme.focus)
        } else {
            Style::default().fg(theme.task)
        });
    frame.render_widget(Paragraph::new(value).block(block), area);
}

fn render_button(frame: &mut Frame<'_>, area: Rect, label: &str, is_focused: bool) {
    let theme = Theme::default();
    let (bg, fg) = if is_focused {
        (theme.focus, Color::Black)
    } else {
        (Color::Reset, theme.task)
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(if is_focused {
            Style::default().fg(theme.focus)
        } else {
            Style::default().fg(theme.secondary)
        })
        .style(Style::default().bg(bg).fg(fg));
    frame.render_widget(
        Paragraph::new(label)
            .alignment(Alignment::Center)
            .block(block),
        area,
    );
}

fn render_checkbox(
    frame: &mut Frame<'_>,
    area: Rect,
    label: &str,
    checked: bool,
    is_focused: bool,
) {
    let theme = Theme::default();
    let check_mark = if checked { "[x]" } else { "[ ]" };
    let style = if is_focused {
        Style::default().fg(theme.focus)
    } else {
        Style::default().fg(theme.task)
    };
    frame.render_widget(
        Paragraph::new(format!("{} {}", check_mark, label)).style(style),
        area,
    );
}

fn render_help_overlay(frame: &mut Frame<'_>) {
    let theme = Theme::default();
    let inner = render_overlay_frame(
        frame,
        OverlayConfig {
            title: " Help ",
            anchor: OverlayAnchor::Center,
            width_percent: 70,
            height_percent: 80,
        },
    );
    let text = [
        "Navigation",
        "  h/l or arrows: switch columns",
        "  j/k or arrows: select task",
        "Task Actions",
        "  n: new task",
        "  Enter: attach selected task",
        "  in task session: Prefix+K returns to previous session",
        "  J/K: move selected task in column",
        "Category Management",
        "  c: add category",
        "  r: rename category",
        "  x: delete category",
        "  H/L: move focused category",
        "Mouse",
        "  left click: focus column or task",
        "  scroll: move through focused column",
        "  click outside this panel: dismiss",
        "General",
        "  Ctrl+P: open command palette",
        "  ?: toggle help",
        "  Esc: dismiss",
        "  q: quit (asks confirmation if sessions are active)",
    ]
    .join("\n");

    frame.render_widget(
        Paragraph::new(text).style(Style::default().fg(theme.task)),
        inner,
    );
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::layout::Rect;

    #[test]
    fn test_calculate_overlay_area_center() {
        let area = Rect::new(0, 0, 100, 100);
        let result = calculate_overlay_area(OverlayAnchor::Center, 50, 50, area);
        assert_eq!(result, Rect::new(25, 25, 50, 50));
    }

    #[test]
    fn test_calculate_overlay_area_top() {
        let area = Rect::new(0, 0, 100, 100);
        let result = calculate_overlay_area(OverlayAnchor::Top, 50, 50, area);
        assert_eq!(result, Rect::new(25, 2, 50, 50));
    }

    #[test]
    fn test_command_palette_overlay_uses_90_percent_width_on_narrow_terminal() {
        assert_eq!(command_palette_overlay_size((29, 40)), (90, 50));
        assert_eq!(command_palette_overlay_size((30, 40)), (60, 50));
    }

    #[test]
    fn test_command_palette_hides_results_on_short_terminal() {
        assert!(!should_render_command_palette_results((120, 9)));
        assert!(should_render_command_palette_results((120, 10)));
    }
}
