use std::collections::HashSet;

use chrono::{DateTime, Utc};
use tui_realm_stdlib::{Checkbox, Input, Label, List, Paragraph, Table};
use tuirealm::{
    MockComponent,
    command::{Cmd, Direction as CmdDirection},
    props::{
        Alignment, AttrValue, Attribute, BorderType, Borders, Color, InputType, Style,
        TableBuilder, TextSpan,
    },
    ratatui::{
        Frame,
        layout::{Constraint, Direction, Layout, Rect},
        style::Style as RatatuiStyle,
        text::{Line as RatatuiLine, Span as RatatuiSpan},
        widgets::{
            Block, Borders as RatatuiBorders, Clear, Paragraph as RatatuiParagraph, Scrollbar,
            ScrollbarOrientation, ScrollbarState,
        },
    },
};

use crate::app::interaction::InteractionLayer;
use crate::app::{
    ActiveDialog, App, ArchiveTaskDialogState, CATEGORY_COLOR_PALETTE, CategoryColorField,
    CategoryInputField, CategoryInputMode, ChangeSummaryState, ConfirmCancelField, ContextMenuItem,
    DeleteProjectDialogState, DeleteRepoDialogState, DeleteTaskField, DetailFocus, EditTaskField,
    Message, NewProjectDialogState, NewProjectField, NewTaskField, ProjectDetailCache,
    RenameProjectDialogState, RenameProjectField, RenameRepoDialogState, RenameRepoField,
    RepoPickerTarget, SettingsSection, SidePanelRow, TodoVisualizationMode, View, ViewMode,
    category_color_label,
};
use crate::command_palette::all_commands;
use crate::theme::{Theme, ThemePreset};
use crate::types::{Category, SessionTodoItem, Task};

#[derive(Clone, Copy)]
pub enum OverlayAnchor {
    Center,
    Top,
}

pub fn render(frame: &mut Frame<'_>, app: &mut App) {
    app.interaction_map.clear();

    match app.current_view {
        View::ProjectList => render_project_list(frame, app),
        View::Board => render_board(frame, app),
        View::Settings => render_settings(frame, app),
        View::Archive => render_archive(frame, app),
    }

    if app.active_dialog != ActiveDialog::None {
        render_dialog(frame, app);
    }

    if app.context_menu.is_some() {
        render_context_menu(frame, app);
    }
}

fn render_project_list(frame: &mut Frame<'_>, app: &mut App) {
    let theme = app.theme;
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(2),
        ])
        .split(frame.area());

    let mut header = Label::default()
        .text("opencode-kanban — Select Project")
        .alignment(Alignment::Center)
        .foreground(theme.base.header)
        .background(theme.base.canvas);
    header.view(frame, chunks[0]);

    let content = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(chunks[1]);

    let mut rows = TableBuilder::default();
    for (idx, project) in app.project_list.iter().enumerate() {
        let is_selected = idx == app.selected_project_index;
        let is_hovered = app.hovered_message.as_ref() == Some(&Message::SelectProject(idx));
        let is_active = app
            .current_project_path
            .as_ref()
            .map(|p| p == &project.path)
            .unwrap_or(false);
        let marker = if is_active { "*" } else { " " };
        let name = format!("{} {}", marker, project.name);
        if is_selected || is_hovered {
            rows.add_col(TextSpan::new(name).fg(theme.interactive.focus).bold())
                .add_row();
        } else {
            rows.add_col(TextSpan::from(name)).add_row();
        }
    }
    if app.project_list.is_empty() {
        rows.add_col(TextSpan::from(
            "  No projects found — press n to create one",
        ))
        .add_row();
    }

    let selected = app
        .selected_project_index
        .min(app.project_list.len().saturating_sub(1));
    let mut list = List::default()
        .title("Projects  (* = active)", Alignment::Left)
        .borders(rounded_borders(theme.interactive.focus))
        .foreground(theme.base.text)
        .background(theme.base.canvas)
        .highlighted_color(theme.interactive.focus)
        .highlighted_str("> ")
        .scroll(true)
        .rows(rows.build())
        .selected_line(selected);
    list.attr(Attribute::Focus, AttrValue::Flag(true));
    list.view(frame, content[0]);

    let list_rect = content[0];
    let content_x = list_rect.x + 1;
    let content_y = list_rect.y + 2;
    let content_width = list_rect.width.saturating_sub(2);
    let content_height = list_rect.height.saturating_sub(2);

    for (idx, _project) in app.project_list.iter().enumerate() {
        if idx < content_height as usize {
            let row_rect = Rect::new(content_x, content_y + idx as u16, content_width, 1);
            app.interaction_map.register_click(
                InteractionLayer::Base,
                row_rect,
                Message::SelectProject(idx),
            );
        }
    }

    render_project_detail_panel(frame, content[1], app);

    let mut footer = Label::default()
        .text("n: new  r: rename  x: delete  Enter: open  j/k: navigate  J/K: reorder  q: quit")
        .alignment(Alignment::Center)
        .foreground(theme.base.text_muted)
        .background(theme.base.canvas);
    footer.view(frame, chunks[2]);
}

fn render_project_detail_panel(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let theme = app.theme;

    let selected = app.project_list.get(app.selected_project_index);
    let cache: Option<&ProjectDetailCache> = app.project_detail_cache.as_ref();

    let mut lines: Vec<TextSpan> = Vec::new();

    if let Some(project) = selected {
        lines.push(TextSpan::new("PROJECT").fg(theme.base.header).bold());
        lines.push(TextSpan::new(detail_kv("Name", &project.name)).fg(theme.base.text));

        let path_str = project.path.to_string_lossy();
        let path_short = clamp_text(&path_str, 55);
        lines.push(TextSpan::new(detail_kv("Path", &path_short)).fg(theme.base.text_muted));
        lines.push(TextSpan::new(""));

        if let Some(c) = cache {
            if c.project_name == project.name {
                lines.push(TextSpan::new("CONTENTS").fg(theme.base.header).bold());
                lines.push(
                    TextSpan::new(detail_kv("Tasks", &c.task_count.to_string()))
                        .fg(theme.base.text),
                );
                lines.push(
                    TextSpan::new(detail_kv("Running", &c.running_count.to_string()))
                        .fg(theme.status_color("running")),
                );
                lines.push(
                    TextSpan::new(detail_kv("Repos", &c.repo_count.to_string()))
                        .fg(theme.base.text),
                );
                lines.push(
                    TextSpan::new(detail_kv("Columns", &c.category_count.to_string()))
                        .fg(theme.base.text),
                );
                lines.push(TextSpan::new(""));
                lines.push(TextSpan::new("FILE").fg(theme.base.header).bold());
                lines.push(
                    TextSpan::new(detail_kv("Size", &format!("{} KB", c.file_size_kb)))
                        .fg(theme.base.text_muted),
                );
            }
        } else {
            lines.push(TextSpan::new("  (loading…)").fg(theme.base.text_muted));
        }

        lines.push(TextSpan::new(""));
        lines.push(TextSpan::new("ACTIONS").fg(theme.base.header).bold());
        lines.push(
            TextSpan::new("  Enter open  r rename  x delete  n new  J/K reorder")
                .fg(theme.base.text_muted),
        );
    } else {
        lines.push(TextSpan::new("No project selected").fg(theme.base.text_muted));
    }

    let mut paragraph = Paragraph::default()
        .title("Details", Alignment::Left)
        .borders(rounded_borders(theme.base.text_muted))
        .foreground(theme.base.text)
        .background(theme.base.canvas)
        .wrap(true)
        .text(lines);
    paragraph.view(frame, area);
}

fn render_board(frame: &mut Frame<'_>, app: &mut App) {
    let theme = app.theme;
    let mut canvas = Paragraph::default()
        .background(theme.base.surface)
        .text([TextSpan::from("")]);
    canvas.view(frame, frame.area());

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(0),
            Constraint::Length(2),
        ])
        .split(frame.area());

    render_header(frame, chunks[0], app);
    match app.view_mode {
        ViewMode::Kanban => render_columns(frame, chunks[1], app),
        ViewMode::SidePanel => render_side_panel(frame, chunks[1], app),
    }
    if app.view_mode == ViewMode::SidePanel && app.log_expanded {
        render_log_expanded_overlay(frame, chunks[1], app);
    }
    render_footer(frame, chunks[2], app);
}

fn render_archive(frame: &mut Frame<'_>, app: &App) {
    let theme = app.theme;
    let mut canvas = Paragraph::default()
        .background(theme.base.canvas)
        .text([TextSpan::from("")]);
    canvas.view(frame, frame.area());

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(0),
            Constraint::Length(2),
        ])
        .split(frame.area());

    let mut header = Label::default()
        .text(format!("Archive ({})", app.archived_tasks.len()))
        .alignment(Alignment::Left)
        .foreground(theme.base.header)
        .background(theme.base.canvas);
    header.view(frame, chunks[0]);

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(chunks[1]);

    let mut rows = TableBuilder::default();
    for task in &app.archived_tasks {
        let archived_label = task
            .archived_at
            .as_deref()
            .map(format_archive_time)
            .unwrap_or_else(|| "unknown time".to_string());
        rows.add_col(
            TextSpan::new(format!("{}  {}", archived_label, task.title)).fg(theme.base.text_muted),
        )
        .add_row();
    }
    if app.archived_tasks.is_empty() {
        rows.add_col(TextSpan::from("No archived tasks")).add_row();
    }

    let selected = app
        .archive_selected_index
        .min(app.archived_tasks.len().saturating_sub(1));
    let mut list = List::default()
        .title("Archived Tasks", Alignment::Left)
        .borders(rounded_borders(theme.interactive.focus))
        .foreground(theme.base.text)
        .highlighted_color(theme.interactive.focus)
        .highlighted_str("> ")
        .scroll(true)
        .rows(rows.build())
        .selected_line(selected);
    list.attr(Attribute::Focus, AttrValue::Flag(true));
    list.view(frame, body[0]);

    let details_lines = if let Some(task) = app.archived_tasks.get(selected) {
        let repo_name = app
            .repos
            .iter()
            .find(|repo| repo.id == task.repo_id)
            .map(|repo| repo.name.as_str())
            .unwrap_or("unknown");
        let category_name = app
            .categories
            .iter()
            .find(|category| category.id == task.category_id)
            .map(|category| category.name.as_str())
            .unwrap_or("unknown");
        let archived_formatted = task
            .archived_at
            .as_deref()
            .map(format_archive_time)
            .unwrap_or_else(|| "unknown".to_string());
        vec![
            TextSpan::new("ARCHIVED TASK").fg(theme.base.header).bold(),
            TextSpan::new(detail_kv("Title", task.title.as_str())).fg(theme.base.text),
            TextSpan::new(detail_kv("Repo", repo_name)).fg(theme.base.text),
            TextSpan::new(detail_kv("Branch", task.branch.as_str())).fg(theme.base.text),
            TextSpan::new(detail_kv("Category", category_name)).fg(theme.base.text),
            TextSpan::new(detail_kv("Archived", &archived_formatted)).fg(theme.base.text_muted),
            TextSpan::new(detail_kv(
                "Path",
                task.worktree_path.as_deref().unwrap_or("n/a"),
            ))
            .fg(theme.base.text_muted),
        ]
    } else {
        vec![TextSpan::new("No archived task selected").fg(theme.base.text_muted)]
    };

    let mut details = Paragraph::default()
        .title("Details", Alignment::Left)
        .borders(rounded_borders(theme.interactive.focus))
        .foreground(theme.base.text)
        .background(theme.base.canvas)
        .wrap(true)
        .text(details_lines);
    details.view(frame, body[1]);

    let mut footer = Label::default()
        .text("j/k:select  u:unarchive  d:delete  Esc:back")
        .alignment(Alignment::Center)
        .foreground(theme.base.text_muted)
        .background(theme.base.canvas);
    footer.view(frame, chunks[2]);
}

fn render_header(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let theme = app.theme;
    let sections = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(65), Constraint::Percentage(35)])
        .split(area);

    let project_name =
        resolve_header_project_name(app.current_project_path.as_deref(), &app.project_list);
    let title = header_title(&project_name, app.category_edit_mode);

    let mut left = Label::default()
        .text(title)
        .alignment(Alignment::Left)
        .foreground(theme.base.header)
        .background(theme.base.surface);
    if app.category_edit_mode {
        left = left.modifiers(tuirealm::props::TextModifiers::BOLD);
    }
    left.view(frame, sections[0]);

    let right_text = format!("tasks: {}  refresh: 0.5s", app.tasks.len());
    let mut right = Label::default()
        .text(right_text)
        .alignment(Alignment::Right)
        .foreground(theme.base.text_muted)
        .background(theme.base.surface);
    right.view(frame, sections[1]);
}

fn header_title(project_name: &str, category_edit_mode: bool) -> String {
    if category_edit_mode {
        format!("opencode-kanban [{project_name}] [CATEGORY EDIT]")
    } else {
        format!("opencode-kanban [{project_name}]")
    }
}

fn resolve_header_project_name(
    current_project_path: Option<&std::path::Path>,
    project_list: &[crate::projects::ProjectInfo],
) -> String {
    if let Some(path) = current_project_path {
        if let Some(project) = project_list
            .iter()
            .find(|project| project.path.as_path() == path)
        {
            return project.name.clone();
        }

        if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
            return stem.to_string();
        }
    }

    crate::projects::DEFAULT_PROJECT.to_string()
}

fn render_footer(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let theme = app.theme;
    let notice = if let Some(notice) = &app.footer_notice {
        notice.as_str()
    } else if app.category_edit_mode {
        "EDIT MODE  h/l:nav  H/L:reorder  p:color  r:rename  x:delete  Ctrl+g:exit"
    } else {
        match app.view_mode {
            ViewMode::Kanban => {
                "j/k:select  Ctrl+u/d:half-page  gg/G:top/bottom  n:new  e:edit  a:archive  A:archive view  Enter:attach  t:todo view  Ctrl+P:palette  c/r/x/p:category  H/L move  J/K reorder  v:view"
            }
            ViewMode::SidePanel => {
                "j/k:select  Ctrl+u/d:half-page  gg/G:top/bottom  Space:collapse  e:edit  a:archive  A:archive view  Enter:attach task  t:todo view  c/r/x/p:category  H/L/J/K:move  v:view"
            }
        }
    };

    let mut footer = Label::default()
        .text(notice)
        .alignment(Alignment::Center)
        .foreground(if app.category_edit_mode && app.footer_notice.is_none() {
            theme.base.header
        } else {
            theme.base.text_muted
        })
        .background(theme.base.surface);
    footer.view(frame, area);
}

fn render_columns(frame: &mut Frame<'_>, area: Rect, app: &mut App) {
    let theme = app.theme;
    if app.categories.is_empty() {
        render_empty_state(frame, area, "No categories yet. Press c to add one.", app);
        return;
    }

    let sorted = sorted_categories(app);
    let visible_columns: Vec<(usize, Category, Rect)> = if app.settings.board_alignment_mode
        == "scroll"
    {
        let viewport_width = usize::from(area.width);
        if viewport_width == 0 {
            return;
        }

        let gap = 1usize;
        let configured_width = usize::from(app.settings.scroll_column_width_chars);
        let column_width = effective_scroll_column_width(configured_width, viewport_width);
        let stride = column_width.saturating_add(gap);

        let focused_slot = sorted
            .iter()
            .position(|(column_idx, _)| *column_idx == app.focused_column)
            .unwrap_or(0);
        let viewport_x = focused_viewport_offset(
            app.kanban_viewport_x,
            viewport_width,
            column_width,
            gap,
            sorted.len(),
            focused_slot,
        );
        app.kanban_viewport_x = viewport_x;

        let mut visible = Vec::new();
        for (slot, (column_idx, category)) in sorted.iter().enumerate() {
            let left = slot.saturating_mul(stride);
            let right = left.saturating_add(column_width);
            let viewport_right = viewport_x.saturating_add(viewport_width);
            if left < viewport_right && right > viewport_x {
                let visible_left = left.max(viewport_x);
                let visible_right = right.min(viewport_right);
                let visible_width = visible_right.saturating_sub(visible_left);
                if visible_width == 0 {
                    continue;
                }
                let translated_x = visible_left.saturating_sub(viewport_x);
                let rect = Rect::new(
                    area.x.saturating_add(translated_x as u16),
                    area.y,
                    visible_width as u16,
                    area.height,
                );
                visible.push((*column_idx, (*category).clone(), rect));
            }
        }

        if visible.is_empty() {
            let focused = sorted
                .iter()
                .enumerate()
                .find(|(_, (column_idx, _))| *column_idx == app.focused_column)
                .unwrap_or((0, &sorted[0]));
            let focused_left = focused.0.saturating_mul(stride);
            let translated_x = focused_left.saturating_sub(viewport_x);
            let width = column_width.min(viewport_width) as u16;
            visible.push((
                focused.1.0,
                focused.1.1.clone(),
                Rect::new(
                    area.x.saturating_add(translated_x as u16),
                    area.y,
                    width,
                    area.height,
                ),
            ));
        }

        visible
    } else {
        app.kanban_viewport_x = 0;
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(vec![
                Constraint::Ratio(1, sorted.len() as u32);
                sorted.len()
            ])
            .split(area);
        sorted
            .iter()
            .enumerate()
            .map(|(slot, (column_idx, category))| (*column_idx, (*category).clone(), columns[slot]))
            .collect()
    };

    let mut hit_test_entries: Vec<(Rect, Message, bool)> = Vec::new();

    for (column_idx, category, column_rect) in visible_columns {
        let mut rows = TableBuilder::default();
        let tasks = tasks_for_category(app, category.id);
        let accent = theme.category_accent(category.color.as_deref());
        let row_count = if tasks.is_empty() {
            1
        } else {
            tasks.iter().map(task_tile_lines).sum()
        };
        let viewport_lines = list_inner_height(column_rect);
        let show_scrollbar = viewport_lines > 0 && row_count > viewport_lines;

        let selected_task = app
            .selected_task_per_column
            .get(&column_idx)
            .copied()
            .unwrap_or(0)
            .min(tasks.len().saturating_sub(1));
        let is_focused_column = column_idx == app.focused_column;

        let tile_width = list_inner_width(column_rect).saturating_sub(usize::from(show_scrollbar));
        for (task_index, task) in tasks.iter().enumerate() {
            let is_hovered =
                app.hovered_message.as_ref() == Some(&Message::SelectTask(column_idx, task_index));
            append_task_tile_rows(
                &mut rows,
                app,
                task,
                (is_focused_column && task_index == selected_task) || is_hovered,
                tile_width,
                accent,
            );
        }

        if tasks.is_empty() {
            rows.add_col(TextSpan::from("No tasks")).add_row();
        }

        let selected_line = column_selected_line(tasks.as_slice(), selected_task);

        let mut list = List::default()
            .title(
                format!("{} ({})", category.name, tasks.len()),
                Alignment::Left,
            )
            .borders(rounded_borders(accent))
            .foreground(theme.base.text)
            .background(theme.base.surface)
            .scroll(true)
            .rows(rows.build())
            .selected_line(selected_line)
            .inactive(Style::default().fg(theme.base.text_muted));
        list.attr(
            Attribute::Focus,
            AttrValue::Flag(column_idx == app.focused_column),
        );
        list.view(frame, column_rect);

        let scroll_offset = column_scroll_offset(selected_line, row_count, viewport_lines);
        let col_rect = column_rect;
        let content_x = col_rect.x + 1;
        let content_y = col_rect.y + 2;
        let content_width = col_rect.width.saturating_sub(2);

        if content_width > 0 {
            for (task_index, _task) in tasks.iter().enumerate() {
                let tile_start_line = task_index * 5;
                if tile_start_line >= scroll_offset
                    && tile_start_line < scroll_offset + viewport_lines
                {
                    let visible_y = content_y + (tile_start_line - scroll_offset) as u16;
                    let task_rect = Rect::new(content_x, visible_y, content_width, 5);
                    hit_test_entries.push((
                        task_rect,
                        Message::SelectTask(column_idx, task_index),
                        true,
                    ));
                }
            }
        }

        if col_rect.width > 0 {
            let header_rect = Rect::new(col_rect.x, col_rect.y, col_rect.width, 2);
            hit_test_entries.push((header_rect, Message::FocusColumn(column_idx), false));
        }

        if show_scrollbar {
            let scroll_offset = column_scroll_offset(selected_line, row_count, viewport_lines);
            let mut state = ScrollbarState::new(row_count)
                .position(scrollbar_position_for_offset(
                    scroll_offset,
                    row_count,
                    viewport_lines,
                ))
                .viewport_content_length(viewport_lines);
            let thumb_color = if is_focused_column {
                accent
            } else {
                theme.base.text_muted
            };
            let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(None)
                .end_symbol(None)
                .track_symbol(Some("│"))
                .track_style(RatatuiStyle::default().fg(theme.base.text_muted))
                .thumb_style(RatatuiStyle::default().fg(thumb_color))
                .thumb_symbol("█");
            let scrollbar_area = inset_rect(column_rect, 1, 1);
            if scrollbar_area.width > 0 && scrollbar_area.height > 0 {
                frame.render_stateful_widget(scrollbar, scrollbar_area, &mut state);
            }
        }
    }

    for (rect, message, is_task) in hit_test_entries {
        if is_task {
            app.interaction_map
                .register_task(InteractionLayer::Base, rect, message);
        } else {
            app.interaction_map
                .register_click(InteractionLayer::Base, rect, message);
        }
    }
}

fn render_side_panel(frame: &mut Frame<'_>, area: Rect, app: &mut App) {
    if app.categories.is_empty() {
        render_empty_state(frame, area, "No categories yet. Press c to add one.", app);
        return;
    }
    let rows = app.side_panel_rows();
    if rows.is_empty() {
        render_empty_state(frame, area, "No tasks available.", app);
        return;
    }

    let width = app.side_panel_width.clamp(20, 80);
    let sections = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(width),
            Constraint::Percentage(100 - width),
        ])
        .split(area);

    render_side_panel_list(frame, sections[0], app, &rows);
    render_side_panel_details(frame, sections[1], app, &rows);
}

fn render_side_panel_list(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &mut App,
    rows_data: &[SidePanelRow],
) {
    let theme = app.theme;
    if rows_data.is_empty() {
        return;
    }

    let selected_row = app
        .side_panel_selected_row
        .min(rows_data.len().saturating_sub(1));
    let row_count = rows_data.iter().map(side_panel_row_lines).sum::<usize>();
    let viewport_lines = list_inner_height(area);
    let show_scrollbar = viewport_lines > 0 && row_count > viewport_lines;
    let tile_width = list_inner_width(area).saturating_sub(usize::from(show_scrollbar));
    let mut rows = TableBuilder::default();
    for (row_index, row) in rows_data.iter().enumerate() {
        let is_hovered =
            app.hovered_message.as_ref() == Some(&Message::SelectTaskInSidePanel(row_index));
        match row {
            SidePanelRow::CategoryHeader {
                category_name,
                category_color,
                visible_tasks,
                total_tasks,
                collapsed,
                ..
            } => {
                let accent = theme.category_accent(category_color.as_deref());
                let marker = if *collapsed { ">" } else { "v" };
                let text = format!("{marker} {category_name} ({visible_tasks}/{total_tasks})");
                let line = pad_to_width(&format!(" {text}"), tile_width);
                let style = if row_index == selected_row || is_hovered {
                    theme.tile_colors(true)
                } else {
                    theme.tile_colors(false)
                };
                rows.add_col(TextSpan::new(line).fg(accent).bg(style.background).bold())
                    .add_row();
            }
            SidePanelRow::Task { task, .. } => {
                append_task_tile_rows(
                    &mut rows,
                    app,
                    task,
                    row_index == selected_row || is_hovered,
                    tile_width,
                    theme.interactive.selected_border,
                );
            }
        }
    }

    let selected_line = side_panel_selected_line(rows_data, selected_row);
    let mut list = List::default()
        .title("Tasks by Category", Alignment::Left)
        .borders(rounded_borders(theme.interactive.focus))
        .foreground(theme.base.text)
        .background(theme.base.surface)
        .scroll(true)
        .rows(rows.build())
        .selected_line(selected_line)
        .inactive(Style::default().fg(theme.base.text_muted));
    list.attr(
        Attribute::Focus,
        AttrValue::Flag(app.detail_focus == DetailFocus::List),
    );
    list.view(frame, area);

    app.interaction_map.register_click(
        InteractionLayer::Base,
        area,
        Message::FocusSidePanel(DetailFocus::List),
    );

    let selected_line = side_panel_selected_line(rows_data, selected_row);
    let scroll_offset = column_scroll_offset(selected_line, row_count, viewport_lines);
    let content_x = area.x.saturating_add(1);
    let content_y = area.y.saturating_add(1);
    let content_width = area.width.saturating_sub(2);

    if content_width > 0 && viewport_lines > 0 {
        let viewport_start = scroll_offset;
        let viewport_end = scroll_offset + viewport_lines;
        let mut row_start = 0usize;

        for (row_index, row) in rows_data.iter().enumerate() {
            let row_height = side_panel_row_lines(row);
            let row_end = row_start + row_height;
            let visible_start = row_start.max(viewport_start);
            let visible_end = row_end.min(viewport_end);
            if visible_end > visible_start {
                let y = content_y + (visible_start - viewport_start) as u16;
                let h = (visible_end - visible_start) as u16;
                app.interaction_map.register_click(
                    InteractionLayer::Base,
                    Rect::new(content_x, y, content_width, h),
                    Message::SelectTaskInSidePanel(row_index),
                );
            }
            row_start = row_end;
        }
    }

    if show_scrollbar {
        let mut state = ScrollbarState::new(row_count)
            .position(scrollbar_position_for_offset(
                scroll_offset,
                row_count,
                viewport_lines,
            ))
            .viewport_content_length(viewport_lines);
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(None)
            .end_symbol(None)
            .track_symbol(Some("│"))
            .track_style(RatatuiStyle::default().fg(theme.base.text_muted))
            .thumb_style(RatatuiStyle::default().fg(theme.interactive.focus))
            .thumb_symbol("█");
        let scrollbar_area = inset_rect(area, 1, 1);
        if scrollbar_area.width > 0 && scrollbar_area.height > 0 {
            frame.render_stateful_widget(scrollbar, scrollbar_area, &mut state);
        }
    }
}

fn render_side_panel_details(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &mut App,
    rows_data: &[SidePanelRow],
) {
    if rows_data.is_empty() {
        return;
    }
    let selected_row = app
        .side_panel_selected_row
        .min(rows_data.len().saturating_sub(1));
    match &rows_data[selected_row] {
        SidePanelRow::Task { task, .. } => render_side_panel_task_details(frame, area, app, task),
        SidePanelRow::CategoryHeader { .. } => {
            render_side_panel_category_details(frame, area, app, &rows_data[selected_row])
        }
    }
}

fn render_side_panel_task_details(frame: &mut Frame<'_>, area: Rect, app: &mut App, task: &Task) {
    let theme = app.theme;

    let split = app.log_split_ratio.clamp(35, 80);
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(split),
            Constraint::Percentage(100 - split),
        ])
        .split(area);

    let repo_name = app
        .repos
        .iter()
        .find(|repo| repo.id == task.repo_id)
        .map(|repo| repo.name.clone())
        .unwrap_or_else(|| "unknown".to_string());
    let runtime_status = task.tmux_status.to_ascii_uppercase();
    let todo_summary = app
        .session_todo_summary(task.id)
        .map(|(done, total)| format!("{done}/{total}"))
        .unwrap_or_else(|| "--".to_string());
    let todo_view = app.todo_visualization_mode.as_str();
    let session = task
        .opencode_session_id
        .as_deref()
        .map(|session_id| {
            app.opencode_session_title(session_id)
                .unwrap_or_else(|| session_id.to_string())
        })
        .unwrap_or_else(|| "n/a".to_string());

    let mut lines: Vec<Vec<TextSpan>> = vec![
        vec![TextSpan::new("OVERVIEW").fg(theme.base.header).bold()],
        vec![TextSpan::new(detail_kv("Title", &task.title)).fg(theme.base.text)],
        vec![
            TextSpan::new(format!("{:>8}: ", "Target")).fg(theme.base.text),
            TextSpan::new(repo_name.clone()).fg(theme.tile.repo),
            TextSpan::new(":").fg(theme.base.text_muted),
            TextSpan::new(task.branch.clone()).fg(theme.tile.branch),
        ],
        vec![TextSpan::new("")],
        vec![TextSpan::new("RUNTIME").fg(theme.base.header).bold()],
        vec![
            TextSpan::new(detail_kv("Status", &runtime_status))
                .fg(theme.status_color(task.tmux_status.as_str())),
        ],
        vec![TextSpan::new(detail_kv("Todos", &todo_summary)).fg(theme.tile.todo)],
        vec![TextSpan::new(detail_kv("TodoView", todo_view)).fg(theme.base.text_muted)],
        vec![TextSpan::new(detail_kv("Session", &session)).fg(theme.base.text)],
    ];

    if app.todo_visualization_mode == TodoVisualizationMode::Checklist {
        let task_todos = app.session_todos(task.id);
        let checklist_lines = todo_checklist_lines(&task_todos);
        if !checklist_lines.is_empty() {
            lines.push(vec![TextSpan::new("")]);
            lines.push(vec![
                TextSpan::new("WORK PLAN").fg(theme.base.header).bold(),
            ]);
            for (line, state) in checklist_lines {
                lines.push(vec![TextSpan::new(line).fg(todo_state_color(theme, state))]);
            }
        }
    }

    let subagents = app.session_subagent_summaries(task.id);
    if !subagents.is_empty() {
        lines.push(vec![TextSpan::new("")]);
        lines.push(vec![
            TextSpan::new("LIVE SUBAGENTS").fg(theme.base.header).bold(),
        ]);
        for subagent in &subagents {
            let title = clamp_text(subagent.title.as_str(), 48);
            let todo = subagent
                .todo_summary
                .map(|(done, total)| format!("{done}/{total}"))
                .unwrap_or_else(|| "--".to_string());
            let spinner = status_spinner_ascii("running", app.pulse_phase);
            lines.push(vec![
                TextSpan::new(format!("{spinner} ")).fg(theme.status.running),
                TextSpan::new(title).fg(theme.base.text),
                TextSpan::new("  todo ").fg(theme.tile.todo),
                TextSpan::new(todo).fg(subagent_count_color(theme, subagent.todo_summary)),
            ]);
        }
    }

    lines.push(vec![TextSpan::new("")]);
    lines.push(vec![TextSpan::new("CHANGES").fg(theme.base.header).bold()]);
    if let Some(summary) = app.current_change_summary.as_ref() {
        const CHANGE_LABEL_WIDTH: usize = 11;
        let change_indent = " ".repeat(CHANGE_LABEL_WIDTH + 2);

        lines.push(vec![
            TextSpan::new(format!("{:>width$}: ", "Base", width = CHANGE_LABEL_WIDTH))
                .fg(theme.base.text_muted),
            TextSpan::new(summary.base_ref.clone()).fg(theme.base.text),
        ]);
        lines.push(vec![
            TextSpan::new(format!(
                "{:>width$}: ",
                "CommitsAhead",
                width = CHANGE_LABEL_WIDTH
            ))
            .fg(theme.base.text_muted),
            TextSpan::new(summary.commits_ahead.to_string()).fg(theme.base.text),
        ]);
        lines.push(vec![
            TextSpan::new(format!("{:>width$}: ", "Diff", width = CHANGE_LABEL_WIDTH))
                .fg(theme.base.text_muted),
            TextSpan::new(format!("{} files", summary.files_changed)).fg(theme.base.text),
        ]);
        lines.push(vec![
            TextSpan::new(format!("{:>width$}: ", "Delta", width = CHANGE_LABEL_WIDTH))
                .fg(theme.base.text_muted),
            TextSpan::new(format!("+{}", summary.insertions)).fg(theme.status.running),
            TextSpan::new(" / ").fg(theme.base.text_muted),
            TextSpan::new(format!("-{}", summary.deletions)).fg(theme.status.dead),
        ]);
        if summary.top_files.is_empty() {
            lines.push(vec![
                TextSpan::new(format!(
                    "{:>width$}: ",
                    "TopFiles",
                    width = CHANGE_LABEL_WIDTH
                ))
                .fg(theme.tile.todo),
                TextSpan::new("n/a").fg(theme.base.text_muted),
            ]);
        } else {
            let mut top_files = summary
                .top_files
                .iter()
                .take(5)
                .map(|file| clamp_text(file, 64));
            if let Some(first_file) = top_files.next() {
                lines.push(vec![
                    TextSpan::new(format!(
                        "{:>width$}: ",
                        "TopFiles",
                        width = CHANGE_LABEL_WIDTH
                    ))
                    .fg(theme.tile.todo),
                    TextSpan::new(first_file).fg(theme.base.text_muted),
                ]);
            }
            for file in top_files {
                lines.push(vec![
                    TextSpan::new(format!("{change_indent}{file}")).fg(theme.base.text_muted),
                ]);
            }
        }
    } else {
        match &app.current_change_summary_state {
            ChangeSummaryState::Loading => {
                lines.push(vec![
                    TextSpan::new(format!("{:>width$}: ", "Base", width = 11))
                        .fg(theme.base.text_muted),
                    TextSpan::new("loading...").fg(theme.base.text_muted),
                ]);
            }
            ChangeSummaryState::Error(err) => {
                lines.push(vec![
                    TextSpan::new(format!("{:>width$}: ", "Base", width = 11))
                        .fg(theme.base.text_muted),
                    TextSpan::new("error").fg(theme.status.dead),
                ]);
                lines.push(vec![
                    TextSpan::new(format!("{:>width$}  ", "", width = 11))
                        .fg(theme.base.text_muted),
                    TextSpan::new(clamp_text(err, 56)).fg(theme.base.text_muted),
                ]);
            }
            ChangeSummaryState::Ready | ChangeSummaryState::Unavailable => {
                lines.push(vec![
                    TextSpan::new(format!("{:>width$}: ", "Base", width = 11))
                        .fg(theme.base.text_muted),
                    TextSpan::new("n/a").fg(theme.base.text_muted),
                ]);
            }
        }
    }

    lines.push(vec![TextSpan::new("")]);
    lines.push(vec![TextSpan::new("ACTIONS").fg(theme.base.header).bold()]);
    lines.push(vec![
        TextSpan::new(
            "d delete  Tab focus  j/k select  Ctrl+u/d half-page  gg/G top/bottom  e/Enter toggle  +/- resize  f expand",
        )
        .fg(theme.base.text_muted),
    ]);

    let detail_viewport = list_inner_height(sections[0]);
    let detail_line_count = lines.len();
    let detail_offset = if detail_viewport == 0 {
        0
    } else {
        app.detail_scroll_offset
            .min(detail_line_count.saturating_sub(detail_viewport))
    };
    let visible_lines: Vec<Vec<TextSpan>> = if detail_viewport == 0 {
        lines
    } else {
        lines
            .into_iter()
            .skip(detail_offset)
            .take(detail_viewport)
            .collect()
    };

    let detail_border = if app.detail_focus == DetailFocus::Details {
        theme.interactive.focus
    } else {
        theme.base.text_muted
    };

    let ratatui_lines: Vec<RatatuiLine<'static>> = visible_lines
        .into_iter()
        .map(|line| {
            RatatuiLine::from(
                line.into_iter()
                    .map(|segment| {
                        let fg = if segment.fg == Color::Reset {
                            theme.base.text
                        } else {
                            segment.fg
                        };
                        let bg = if segment.bg == Color::Reset {
                            theme.base.surface
                        } else {
                            segment.bg
                        };
                        RatatuiSpan::styled(
                            segment.content,
                            RatatuiStyle::default()
                                .fg(fg)
                                .bg(bg)
                                .add_modifier(segment.modifiers),
                        )
                    })
                    .collect::<Vec<_>>(),
            )
        })
        .collect();
    let details_block = Block::default()
        .title("Details")
        .borders(RatatuiBorders::ALL)
        .border_type(tuirealm::ratatui::widgets::BorderType::Rounded)
        .border_style(RatatuiStyle::default().fg(detail_border))
        .style(RatatuiStyle::default().bg(theme.base.surface));
    let paragraph = RatatuiParagraph::new(ratatui_lines)
        .block(details_block)
        .style(
            RatatuiStyle::default()
                .fg(theme.base.text)
                .bg(theme.base.surface),
        );
    frame.render_widget(paragraph, sections[0]);

    if detail_viewport > 0 && detail_line_count > detail_viewport {
        let mut state = ScrollbarState::new(detail_line_count)
            .position(scrollbar_position_for_offset(
                detail_offset,
                detail_line_count,
                detail_viewport,
            ))
            .viewport_content_length(detail_viewport);
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(None)
            .end_symbol(None)
            .track_symbol(Some("│"))
            .track_style(RatatuiStyle::default().fg(theme.base.text_muted))
            .thumb_style(RatatuiStyle::default().fg(detail_border))
            .thumb_symbol("█");
        let scrollbar_area = inset_rect(sections[0], 1, 1);
        if scrollbar_area.width > 0 && scrollbar_area.height > 0 {
            frame.render_stateful_widget(scrollbar, scrollbar_area, &mut state);
        }
    }

    render_log_panel(frame, sections[1], app);

    app.interaction_map.register_click(
        InteractionLayer::Base,
        sections[0],
        Message::FocusSidePanel(DetailFocus::Details),
    );
    app.interaction_map.register_click(
        InteractionLayer::Base,
        sections[1],
        Message::FocusSidePanel(DetailFocus::Log),
    );
}

fn render_log_panel(frame: &mut Frame<'_>, area: Rect, app: &mut App) {
    let theme = app.theme;
    let entries = app
        .current_log_buffer
        .as_deref()
        .map(parse_structured_log_entries)
        .unwrap_or_default();
    let selected_entry = if entries.is_empty() {
        0
    } else {
        app.log_scroll_offset.min(entries.len() - 1)
    };
    let viewport = list_inner_height(area);

    let visible_lines = if entries.is_empty() {
        vec![TextSpan::new("No log output available.").fg(theme.base.text_muted)]
    } else {
        log_entries_to_spans(
            &entries,
            theme,
            &app.log_expanded_entries,
            selected_entry,
            viewport,
        )
    };

    let border = if app.detail_focus == DetailFocus::Log {
        theme.interactive.focus
    } else {
        theme.base.text_muted
    };

    let mut paragraph = Paragraph::default()
        .title("Logs | structured | e/Enter toggle", Alignment::Left)
        .borders(rounded_borders(border))
        .foreground(theme.base.text)
        .background(theme.base.surface)
        .text(visible_lines);
    paragraph.view(frame, area);

    let entry_count = entries.len().max(1);
    let viewport_entries = viewport.min(entry_count).max(1);
    if viewport > 0 && entry_count > viewport_entries {
        let mut state = ScrollbarState::new(entry_count)
            .position(scrollbar_position_for_offset(
                selected_entry,
                entry_count,
                viewport_entries,
            ))
            .viewport_content_length(viewport_entries);
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(None)
            .end_symbol(None)
            .track_symbol(Some("│"))
            .track_style(RatatuiStyle::default().fg(theme.base.text_muted))
            .thumb_style(RatatuiStyle::default().fg(border))
            .thumb_symbol("█");
        let scrollbar_area = inset_rect(area, 1, 1);
        if scrollbar_area.width > 0 && scrollbar_area.height > 0 {
            frame.render_stateful_widget(scrollbar, scrollbar_area, &mut state);
        }
    }
}

fn render_side_panel_category_details(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &mut App,
    row: &SidePanelRow,
) {
    let SidePanelRow::CategoryHeader {
        category_name,
        category_id,
        category_color,
        total_tasks,
        visible_tasks,
        collapsed,
        ..
    } = row
    else {
        return;
    };

    let theme = app.theme;
    let accent = theme.category_accent(category_color.as_deref());
    let (running, idle) = category_status_counts(app, *category_id);

    let mut lines = vec![
        TextSpan::new("CATEGORY").fg(theme.base.header).bold(),
        TextSpan::new(detail_kv("Name", category_name.as_str())).fg(theme.base.text),
        TextSpan::new(detail_kv(
            "State",
            if *collapsed { "collapsed" } else { "expanded" },
        ))
        .fg(theme.base.text),
        TextSpan::new(detail_kv("Visible", &visible_tasks.to_string())).fg(theme.base.text),
        TextSpan::new(detail_kv("Tasks", &total_tasks.to_string())).fg(theme.base.text),
        TextSpan::new(""),
        TextSpan::new("STATUS").fg(theme.base.header).bold(),
        TextSpan::new(detail_kv("Running", &running.to_string())).fg(theme.status_color("running")),
        TextSpan::new(detail_kv("Idle", &idle.to_string())).fg(theme.status_color("idle")),
    ];

    lines.push(TextSpan::new(""));
    lines.push(TextSpan::new("ACTIONS").fg(theme.base.header).bold());
    lines.push(
        TextSpan::new(
            "Space toggle  j/k navigate  Ctrl+u/d half-page  gg/G top/bottom  Enter attach on task",
        )
        .fg(accent),
    );

    let viewport = list_inner_height(area);
    let line_count = lines.len();
    let offset = if viewport == 0 {
        0
    } else {
        app.detail_scroll_offset
            .min(line_count.saturating_sub(viewport))
    };
    let visible_lines = if viewport == 0 {
        lines
    } else {
        lines
            .into_iter()
            .skip(offset)
            .take(viewport)
            .collect::<Vec<_>>()
    };

    let border = if app.detail_focus == DetailFocus::Details {
        theme.interactive.focus
    } else {
        accent
    };

    let mut paragraph = Paragraph::default()
        .title("Category Summary", Alignment::Left)
        .borders(rounded_borders(border))
        .foreground(theme.base.text)
        .background(theme.base.surface)
        .text(visible_lines);
    paragraph.view(frame, area);

    if viewport > 0 && line_count > viewport {
        let mut state = ScrollbarState::new(line_count)
            .position(scrollbar_position_for_offset(offset, line_count, viewport))
            .viewport_content_length(viewport);
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(None)
            .end_symbol(None)
            .track_symbol(Some("│"))
            .track_style(RatatuiStyle::default().fg(theme.base.text_muted))
            .thumb_style(RatatuiStyle::default().fg(border))
            .thumb_symbol("█");
        let scrollbar_area = inset_rect(area, 1, 1);
        if scrollbar_area.width > 0 && scrollbar_area.height > 0 {
            frame.render_stateful_widget(scrollbar, scrollbar_area, &mut state);
        }
    }

    app.interaction_map.register_click(
        InteractionLayer::Base,
        area,
        Message::FocusSidePanel(DetailFocus::Details),
    );
}

fn render_dialog(frame: &mut Frame<'_>, app: &mut App) {
    if matches!(app.active_dialog, ActiveDialog::Help) {
        render_help_overlay(frame, app);
        return;
    }

    let (width_percent, height_percent) = match &app.active_dialog {
        ActiveDialog::CommandPalette(_) => command_palette_overlay_size(app.viewport),
        ActiveDialog::NewTask(_) => (80, 72),
        ActiveDialog::ArchiveTask(_) => (55, 35),
        ActiveDialog::DeleteTask(_) => (60, 60),
        ActiveDialog::EditTask(_) => (70, 45),
        ActiveDialog::CategoryInput(_) => (60, 40),
        ActiveDialog::CategoryColor(_) => (60, 58),
        ActiveDialog::DeleteCategory(_) => (60, 40),
        ActiveDialog::NewProject(_) => (60, 40),
        ActiveDialog::RenameProject(_) => (60, 40),
        ActiveDialog::DeleteProject(_) => (60, 35),
        ActiveDialog::RenameRepo(_) => (60, 40),
        ActiveDialog::DeleteRepo(_) => (60, 35),
        _ => (60, 45),
    };
    let anchor = if matches!(app.active_dialog, ActiveDialog::CommandPalette(_)) {
        OverlayAnchor::Top
    } else {
        OverlayAnchor::Center
    };

    let dialog_area = calculate_overlay_area(anchor, width_percent, height_percent, frame.area());
    frame.render_widget(Clear, dialog_area);

    match app.active_dialog.clone() {
        ActiveDialog::NewTask(state) => {
            render_new_task_dialog(frame, dialog_area, app, &state);
            if let Some(picker) = &state.repo_picker {
                render_repo_picker_dialog(frame, app, picker);
            }
        }
        ActiveDialog::DeleteTask(state) => {
            render_delete_task_dialog(frame, dialog_area, app, &state)
        }
        ActiveDialog::EditTask(state) => render_edit_task_dialog(frame, dialog_area, app, &state),
        ActiveDialog::ArchiveTask(state) => {
            render_archive_task_dialog(frame, dialog_area, app, &state)
        }
        ActiveDialog::CategoryInput(state) => {
            render_category_dialog(frame, dialog_area, app, &state)
        }
        ActiveDialog::CategoryColor(state) => {
            render_category_color_dialog(frame, dialog_area, app, &state)
        }
        ActiveDialog::DeleteCategory(state) => {
            render_delete_category_dialog(frame, dialog_area, app, &state)
        }
        ActiveDialog::Error(state) => {
            let text = format!("{}\n\n{}", state.title, state.detail);
            render_error_dialog(frame, dialog_area, app, "Error", &text);
        }
        ActiveDialog::WorktreeNotFound(state) => {
            let text = format!(
                "Worktree missing for task '{}'.\n\nEnter: recreate  m: mark idle  Esc: cancel",
                state.task_title
            );
            render_message_dialog(frame, dialog_area, app, "Worktree Not Found", &text);
        }
        ActiveDialog::RepoUnavailable(state) => {
            let text = format!(
                "Repository unavailable for '{}'.\nPath: {}\n\nPress Enter or Esc.",
                state.task_title, state.repo_path
            );
            render_message_dialog(frame, dialog_area, app, "Repository Unavailable", &text);
        }
        ActiveDialog::ConfirmQuit(state) => {
            render_confirm_quit_dialog(frame, dialog_area, app, &state);
        }
        ActiveDialog::CommandPalette(state) => {
            render_command_palette_dialog(frame, dialog_area, app, &state)
        }
        ActiveDialog::NewProject(state) => {
            render_new_project_dialog(frame, dialog_area, app, &state)
        }
        ActiveDialog::RenameProject(state) => {
            render_rename_project_dialog(frame, dialog_area, app, &state)
        }
        ActiveDialog::DeleteProject(state) => {
            render_delete_project_dialog(frame, dialog_area, app, &state)
        }
        ActiveDialog::RenameRepo(state) => {
            render_rename_repo_dialog(frame, dialog_area, app, &state)
        }
        ActiveDialog::DeleteRepo(state) => {
            render_delete_repo_dialog(frame, dialog_area, app, &state)
        }
        ActiveDialog::MoveTask(_) | ActiveDialog::None | ActiveDialog::Help => {}
    }
}

fn render_new_task_dialog(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &mut App,
    state: &crate::app::NewTaskDialogState,
) {
    let theme = app.theme;
    let surface = dialog_surface(theme);

    let mut panel =
        dialog_panel("New Task", Alignment::Center, theme, surface).text([TextSpan::from("")]);
    panel.view(frame, area);

    let panel_inner = inset_rect(area, 1, 1);
    let layout = if state.use_existing_directory {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Length(2),
                Constraint::Min(0),
            ])
            .split(panel_inner)
    } else {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Length(2),
                Constraint::Min(0),
            ])
            .split(panel_inner)
    };

    let mode_focused = state.focused_field == NewTaskField::UseExistingDirectory;
    let mut mode_panel = Paragraph::default()
        .title("Mode", Alignment::Left)
        .borders(dialog_border(theme))
        .foreground(if mode_focused {
            theme.interactive.focus
        } else {
            theme.base.text
        })
        .background(surface)
        .text([TextSpan::from("")]);
    mode_panel.view(frame, layout[0]);

    let mode_inner = inset_rect(layout[0], 1, 1);
    render_mode_segmented_control(
        frame,
        mode_inner,
        app,
        mode_focused,
        state.use_existing_directory,
    );

    let left_width = mode_inner.width / 2;
    let right_width = mode_inner.width.saturating_sub(left_width);
    let mode_click_regions = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(left_width),
            Constraint::Length(right_width),
        ])
        .split(mode_inner);

    app.interaction_map.register_click(
        InteractionLayer::Dialog,
        mode_click_regions[0],
        Message::SetNewTaskUseExistingDirectory(false),
    );
    app.interaction_map.register_click(
        InteractionLayer::Dialog,
        mode_click_regions[1],
        Message::SetNewTaskUseExistingDirectory(true),
    );

    if state.use_existing_directory {
        let directory_picker_editing = state
            .repo_picker
            .as_ref()
            .is_some_and(|picker| picker.target == RepoPickerTarget::ExistingDirectory);
        render_repo_picker_input_component(
            frame,
            layout[1],
            "Directory",
            &state.existing_dir_input,
            directory_picker_editing,
            state.focused_field == NewTaskField::ExistingDirectory,
            surface,
            theme,
        );
        app.interaction_map.register_click(
            InteractionLayer::Dialog,
            layout[1],
            Message::FocusNewTaskField(NewTaskField::ExistingDirectory),
        );

        render_input_component(
            frame,
            layout[2],
            "Title",
            &state.title_input,
            state.focused_field == NewTaskField::Title,
            theme,
            Some("defaults to branch"),
        );
        app.interaction_map.register_click(
            InteractionLayer::Dialog,
            layout[2],
            Message::FocusNewTaskField(NewTaskField::Title),
        );

        let actions = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(layout[3]);

        render_action_button(
            frame,
            actions[0],
            "Create",
            matches!(state.focused_field, NewTaskField::Create),
            false,
            app,
            Some(Message::CreateTask),
        );
        render_action_button(
            frame,
            actions[1],
            "Cancel",
            matches!(state.focused_field, NewTaskField::Cancel),
            false,
            app,
            Some(Message::DismissDialog),
        );

        match state.focused_field {
            NewTaskField::ExistingDirectory => {
                set_text_input_cursor(frame, layout[1], &state.existing_dir_input)
            }
            NewTaskField::Title => set_text_input_cursor(frame, layout[2], &state.title_input),
            _ => {}
        }
    } else {
        let repo_display = if state.repo_input.trim().is_empty() {
            app.repos
                .get(state.repo_idx)
                .map(|repo| repo.path.as_str())
                .unwrap_or("")
        } else {
            state.repo_input.as_str()
        };
        let repo_picker_editing = state
            .repo_picker
            .as_ref()
            .is_some_and(|picker| picker.target == RepoPickerTarget::Repo);

        render_repo_picker_input_component(
            frame,
            layout[1],
            "Repo",
            repo_display,
            repo_picker_editing,
            state.focused_field == NewTaskField::Repo,
            surface,
            theme,
        );
        app.interaction_map.register_click(
            InteractionLayer::Dialog,
            layout[1],
            Message::FocusNewTaskField(NewTaskField::Repo),
        );

        render_input_component(
            frame,
            layout[2],
            "Branch",
            &state.branch_input,
            state.focused_field == NewTaskField::Branch,
            theme,
            Some("auto-generated if empty"),
        );
        app.interaction_map.register_click(
            InteractionLayer::Dialog,
            layout[2],
            Message::FocusNewTaskField(NewTaskField::Branch),
        );
        render_input_component(
            frame,
            layout[3],
            "Base",
            &state.base_input,
            state.focused_field == NewTaskField::Base,
            theme,
            None,
        );
        app.interaction_map.register_click(
            InteractionLayer::Dialog,
            layout[3],
            Message::FocusNewTaskField(NewTaskField::Base),
        );
        render_input_component(
            frame,
            layout[4],
            "Title",
            &state.title_input,
            state.focused_field == NewTaskField::Title,
            theme,
            Some("defaults to branch"),
        );
        app.interaction_map.register_click(
            InteractionLayer::Dialog,
            layout[4],
            Message::FocusNewTaskField(NewTaskField::Title),
        );

        let selected = if state.ensure_base_up_to_date {
            vec![0]
        } else {
            Vec::new()
        };
        let options_focused = state.focused_field == NewTaskField::EnsureBaseUpToDate;
        let options_foreground = if options_focused {
            theme.interactive.focus
        } else {
            theme.base.text
        };
        let mut checkbox = dialog_checkbox("Options", theme, surface)
            .foreground(options_foreground)
            .choices(["Ensure base is up to date"])
            .values(&selected)
            .rewind(false);
        checkbox.attr(Attribute::Focus, AttrValue::Flag(options_focused));
        checkbox.view(frame, layout[5]);
        app.interaction_map.register_click(
            InteractionLayer::Dialog,
            layout[5],
            Message::ToggleNewTaskCheckbox,
        );

        let actions = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(layout[6]);

        render_action_button(
            frame,
            actions[0],
            "Create",
            matches!(state.focused_field, NewTaskField::Create),
            false,
            app,
            Some(Message::CreateTask),
        );
        render_action_button(
            frame,
            actions[1],
            "Cancel",
            matches!(state.focused_field, NewTaskField::Cancel),
            false,
            app,
            Some(Message::DismissDialog),
        );

        match state.focused_field {
            NewTaskField::Branch => set_text_input_cursor(frame, layout[2], &state.branch_input),
            NewTaskField::Base => set_text_input_cursor(frame, layout[3], &state.base_input),
            NewTaskField::Title => set_text_input_cursor(frame, layout[4], &state.title_input),
            _ => {}
        }
    }
}

fn render_delete_task_dialog(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &mut App,
    state: &crate::app::DeleteTaskDialogState,
) {
    let theme = app.theme;
    let panel_inner = inset_rect(area, 1, 1);
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(3),
        ])
        .split(panel_inner);

    let mut panel = dialog_panel("Delete Task", Alignment::Center, theme, theme.base.canvas)
        .text([TextSpan::from("")]);
    panel.view(frame, area);

    let mut summary = Paragraph::default()
        .foreground(theme.base.text)
        .background(theme.base.canvas)
        .wrap(true)
        .text([
            TextSpan::from(format!(
                "Delete task '{}' ({})",
                state.task_title, state.task_branch
            )),
            TextSpan::from(if state.remove_worktree || state.delete_branch {
                if state.confirm_destructive {
                    "Press Delete again to confirm worktree/branch cleanup."
                } else {
                    "Destructive cleanup selected. First Delete arms confirmation."
                }
            } else {
                "Use Space/Enter to toggle options."
            }),
        ]);
    summary.view(frame, layout[0]);

    let selected = [
        (state.kill_tmux, 0usize),
        (state.remove_worktree, 1usize),
        (state.delete_branch, 2usize),
    ]
    .into_iter()
    .filter_map(|(enabled, idx)| enabled.then_some(idx))
    .collect::<Vec<_>>();

    let delete_options_focused = matches!(
        state.focused_field,
        DeleteTaskField::KillTmux | DeleteTaskField::RemoveWorktree | DeleteTaskField::DeleteBranch
    );
    let delete_options_foreground = if delete_options_focused {
        theme.interactive.focus
    } else {
        theme.base.text
    };
    let mut checkbox = dialog_checkbox("Delete Options", theme, dialog_surface(theme))
        .foreground(delete_options_foreground)
        .choices(["Kill tmux", "Remove worktree", "Delete branch"])
        .values(&selected)
        .rewind(false);
    checkbox.attr(Attribute::Focus, AttrValue::Flag(delete_options_focused));
    set_checkbox_highlight_choice(
        &mut checkbox,
        delete_task_checkbox_focus_index(state.focused_field),
    );
    checkbox.view(frame, layout[1]);

    let delete_option_click_regions = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(34),
            Constraint::Percentage(33),
            Constraint::Percentage(33),
        ])
        .split(layout[1]);
    app.interaction_map.register_click(
        InteractionLayer::Dialog,
        delete_option_click_regions[0],
        Message::ToggleDeleteTaskCheckbox(DeleteTaskField::KillTmux),
    );
    app.interaction_map.register_click(
        InteractionLayer::Dialog,
        delete_option_click_regions[1],
        Message::ToggleDeleteTaskCheckbox(DeleteTaskField::RemoveWorktree),
    );
    app.interaction_map.register_click(
        InteractionLayer::Dialog,
        delete_option_click_regions[2],
        Message::ToggleDeleteTaskCheckbox(DeleteTaskField::DeleteBranch),
    );

    let buttons = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(layout[3]);

    let delete_label =
        if (state.remove_worktree || state.delete_branch) && state.confirm_destructive {
            "Confirm Delete"
        } else {
            "Delete"
        };

    render_action_button(
        frame,
        buttons[0],
        delete_label,
        matches!(state.focused_field, DeleteTaskField::Delete),
        true,
        app,
        Some(Message::ConfirmDeleteTask),
    );
    render_action_button(
        frame,
        buttons[1],
        "Cancel",
        matches!(state.focused_field, DeleteTaskField::Cancel),
        false,
        app,
        Some(Message::DismissDialog),
    );
}

fn render_edit_task_dialog(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &mut App,
    state: &crate::app::EditTaskDialogState,
) {
    let theme = app.theme;
    let surface = dialog_surface(theme);

    let mut panel =
        dialog_panel("Edit Task", Alignment::Center, theme, surface).text([TextSpan::from("")]);
    panel.view(frame, area);

    let panel_inner = inset_rect(area, 1, 1);
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(2),
            Constraint::Length(3),
            Constraint::Length(2),
            Constraint::Min(0),
        ])
        .split(panel_inner);

    render_input_component(
        frame,
        layout[0],
        "Repo",
        &state.repo_path,
        false,
        theme,
        None,
    );
    render_input_component(
        frame,
        layout[1],
        "Branch",
        &state.branch,
        false,
        theme,
        None,
    );
    render_input_component(
        frame,
        layout[2],
        "Title",
        &state.title_input,
        matches!(state.focused_field, EditTaskField::Title),
        theme,
        None,
    );
    app.interaction_map.register_click(
        InteractionLayer::Dialog,
        layout[2],
        Message::FocusEditTaskField(EditTaskField::Title),
    );

    let mut read_only_hint = Label::default()
        .text("Repo and branch are read-only in this dialog")
        .alignment(Alignment::Center)
        .foreground(theme.base.text_muted)
        .background(surface);
    read_only_hint.view(frame, layout[3]);

    let buttons = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(layout[4]);

    render_action_button(
        frame,
        buttons[0],
        "Save",
        matches!(state.focused_field, EditTaskField::Save),
        false,
        app,
        Some(Message::ConfirmEditTask),
    );
    render_action_button(
        frame,
        buttons[1],
        "Cancel",
        matches!(state.focused_field, EditTaskField::Cancel),
        false,
        app,
        Some(Message::DismissDialog),
    );

    let mut hint = Label::default()
        .text("Tab: next field  Enter: confirm  Esc: cancel")
        .alignment(Alignment::Center)
        .foreground(theme.base.text_muted)
        .background(surface);
    hint.view(frame, layout[5]);

    if matches!(state.focused_field, EditTaskField::Title) {
        set_text_input_cursor(frame, layout[2], &state.title_input);
    }
}

fn render_archive_task_dialog(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &mut App,
    state: &ArchiveTaskDialogState,
) {
    let text = format!("Archive task '{}' ?", state.task_title);
    render_confirm_cancel_dialog(
        frame,
        area,
        app,
        ConfirmCancelDialogSpec {
            title: "Archive Task",
            text: &text,
            confirm_label: "Archive",
            confirm_destructive: false,
            focused_field: state.focused_field,
            confirm_message: Message::ConfirmArchiveTask,
            cancel_message: Message::DismissDialog,
        },
    );
}

fn render_category_dialog(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &mut App,
    state: &crate::app::CategoryInputDialogState,
) {
    let theme = app.theme;
    let surface = dialog_surface(theme);

    let title = match state.mode {
        CategoryInputMode::Add => "Add Category",
        CategoryInputMode::Rename => "Rename Category",
    };

    let mut panel =
        dialog_panel(title, Alignment::Center, theme, surface).text([TextSpan::from("")]);
    panel.view(frame, area);

    let panel_inner = inset_rect(area, 1, 1);
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(2),
            Constraint::Length(3),
            Constraint::Length(2),
            Constraint::Min(0),
        ])
        .split(panel_inner);

    render_input_component(
        frame,
        layout[0],
        "Name",
        &state.name_input,
        matches!(state.focused_field, CategoryInputField::Name),
        theme,
        None,
    );
    app.interaction_map.register_click(
        InteractionLayer::Dialog,
        layout[0],
        Message::FocusCategoryInputField(CategoryInputField::Name),
    );

    let buttons = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(layout[2]);

    render_action_button(
        frame,
        buttons[0],
        if state.mode == CategoryInputMode::Add {
            "Add"
        } else {
            "Rename"
        },
        matches!(state.focused_field, CategoryInputField::Confirm),
        false,
        app,
        Some(Message::SubmitCategoryInput),
    );
    render_action_button(
        frame,
        buttons[1],
        "Cancel",
        matches!(state.focused_field, CategoryInputField::Cancel),
        false,
        app,
        Some(Message::DismissDialog),
    );

    let mut hint = Label::default()
        .text("Tab: next field  Enter: confirm  Esc: cancel")
        .alignment(Alignment::Center)
        .foreground(theme.base.text_muted)
        .background(surface);
    hint.view(frame, layout[3]);

    if matches!(state.focused_field, CategoryInputField::Name) {
        set_text_input_cursor(frame, layout[0], &state.name_input);
    }
}

fn render_delete_category_dialog(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &mut App,
    state: &crate::app::DeleteCategoryDialogState,
) {
    let text = if state.task_count > 0 {
        format!(
            "Category '{}' contains {} tasks.\nEmpty the category before deleting.",
            state.category_name, state.task_count
        )
    } else {
        format!("Delete category '{}' ?", state.category_name)
    };

    render_confirm_cancel_dialog(
        frame,
        area,
        app,
        ConfirmCancelDialogSpec {
            title: "Delete Category",
            text: &text,
            confirm_label: "Delete",
            confirm_destructive: true,
            focused_field: state.focused_field,
            confirm_message: Message::ConfirmDeleteCategory,
            cancel_message: Message::DismissDialog,
        },
    );
}

fn render_confirm_quit_dialog(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &mut App,
    state: &crate::app::ConfirmQuitDialogState,
) {
    let text = format!(
        "{} active sessions detected.\nQuit anyway?",
        state.active_session_count
    );
    render_confirm_cancel_dialog(
        frame,
        area,
        app,
        ConfirmCancelDialogSpec {
            title: "Confirm Quit",
            text: &text,
            confirm_label: "Quit",
            confirm_destructive: true,
            focused_field: state.focused_field,
            confirm_message: Message::ConfirmQuit,
            cancel_message: Message::DismissDialog,
        },
    );
}

struct ConfirmCancelDialogSpec<'a> {
    title: &'a str,
    text: &'a str,
    confirm_label: &'a str,
    confirm_destructive: bool,
    focused_field: ConfirmCancelField,
    confirm_message: Message,
    cancel_message: Message,
}

fn render_confirm_cancel_dialog(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &mut App,
    spec: ConfirmCancelDialogSpec<'_>,
) {
    let theme = app.theme;
    let surface = dialog_surface(theme);

    let mut panel =
        dialog_panel(spec.title, Alignment::Center, theme, surface).text([TextSpan::from("")]);
    panel.view(frame, area);

    let panel_inner = inset_rect(area, 1, 1);
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),
            Constraint::Length(3),
            Constraint::Length(2),
            Constraint::Min(0),
        ])
        .split(panel_inner);

    let mut summary = Paragraph::default()
        .foreground(theme.base.text)
        .background(surface)
        .wrap(true)
        .alignment(Alignment::Center)
        .text([TextSpan::from(spec.text.to_string())]);
    summary.view(frame, layout[0]);

    let buttons = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(layout[1]);

    render_action_button(
        frame,
        buttons[0],
        spec.confirm_label,
        matches!(spec.focused_field, ConfirmCancelField::Confirm),
        spec.confirm_destructive,
        app,
        Some(spec.confirm_message.clone()),
    );
    render_action_button(
        frame,
        buttons[1],
        "Cancel",
        matches!(spec.focused_field, ConfirmCancelField::Cancel),
        false,
        app,
        Some(spec.cancel_message.clone()),
    );

    let mut hint = Label::default()
        .text("Tab/Arrows/hjkl: switch  Enter: confirm  Esc: cancel")
        .alignment(Alignment::Center)
        .foreground(theme.base.text_muted)
        .background(surface);
    hint.view(frame, layout[2]);
}

fn render_new_project_dialog(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &mut App,
    state: &NewProjectDialogState,
) {
    let theme = app.theme;
    let surface = dialog_surface(theme);

    let mut panel =
        dialog_panel("New Project", Alignment::Center, theme, surface).text([TextSpan::from("")]);
    panel.view(frame, area);

    let panel_inner = inset_rect(area, 1, 1);
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(2),
            Constraint::Length(3),
            Constraint::Length(2),
            Constraint::Min(0),
        ])
        .split(panel_inner);

    render_input_component(
        frame,
        layout[0],
        "Name",
        &state.name_input,
        matches!(state.focused_field, NewProjectField::Name),
        theme,
        None,
    );
    app.interaction_map.register_click(
        InteractionLayer::Dialog,
        layout[0],
        Message::FocusNewProjectField(NewProjectField::Name),
    );

    let buttons = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(layout[2]);

    render_action_button(
        frame,
        buttons[0],
        "Create",
        matches!(state.focused_field, NewProjectField::Create),
        false,
        app,
        Some(Message::CreateProject),
    );
    render_action_button(
        frame,
        buttons[1],
        "Cancel",
        matches!(state.focused_field, NewProjectField::Cancel),
        false,
        app,
        Some(Message::DismissDialog),
    );

    let mut hint = Label::default()
        .text("Tab: next field  Enter: confirm  Esc: cancel")
        .alignment(Alignment::Center)
        .foreground(theme.base.text_muted)
        .background(surface);
    hint.view(frame, layout[3]);

    if matches!(state.focused_field, NewProjectField::Name) {
        set_text_input_cursor(frame, layout[0], &state.name_input);
    }
}

fn render_rename_project_dialog(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &mut App,
    state: &RenameProjectDialogState,
) {
    let theme = app.theme;
    let surface = dialog_surface(theme);

    let mut panel = dialog_panel("Rename Project", Alignment::Center, theme, surface)
        .text([TextSpan::from("")]);
    panel.view(frame, area);

    let panel_inner = inset_rect(area, 1, 1);
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(2),
            Constraint::Length(3),
            Constraint::Length(2),
            Constraint::Min(0),
        ])
        .split(panel_inner);

    render_input_component(
        frame,
        layout[0],
        "New Name",
        &state.name_input,
        matches!(state.focused_field, RenameProjectField::Name),
        theme,
        None,
    );
    app.interaction_map.register_click(
        InteractionLayer::Dialog,
        layout[0],
        Message::FocusRenameProjectField(RenameProjectField::Name),
    );

    let buttons = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(layout[2]);

    render_action_button(
        frame,
        buttons[0],
        "Rename",
        matches!(state.focused_field, RenameProjectField::Confirm),
        false,
        app,
        Some(Message::ConfirmRenameProject),
    );
    render_action_button(
        frame,
        buttons[1],
        "Cancel",
        matches!(state.focused_field, RenameProjectField::Cancel),
        false,
        app,
        Some(Message::DismissDialog),
    );

    let mut hint = Label::default()
        .text("Tab: next field  Enter: confirm  Esc: cancel")
        .alignment(Alignment::Center)
        .foreground(theme.base.text_muted)
        .background(surface);
    hint.view(frame, layout[3]);

    if matches!(state.focused_field, RenameProjectField::Name) {
        set_text_input_cursor(frame, layout[0], &state.name_input);
    }
}

fn render_delete_project_dialog(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &mut App,
    state: &DeleteProjectDialogState,
) {
    let theme = app.theme;
    let surface = dialog_surface(theme);

    let mut panel = dialog_panel("Delete Project", Alignment::Center, theme, surface)
        .text([TextSpan::from("")]);
    panel.view(frame, area);

    let panel_inner = inset_rect(area, 1, 1);
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),
            Constraint::Min(1),
            Constraint::Length(3),
            Constraint::Length(2),
        ])
        .split(panel_inner);

    let mut summary = Paragraph::default()
        .foreground(theme.base.text)
        .background(surface)
        .wrap(true)
        .alignment(Alignment::Center)
        .text([TextSpan::from(format!(
            "Permanently delete '{}'?\nAll tasks will be lost.",
            state.project_name
        ))]);
    summary.view(frame, layout[0]);

    let buttons = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(layout[2]);

    render_action_button(
        frame,
        buttons[0],
        "Delete",
        true,
        true,
        app,
        Some(Message::ConfirmDeleteProject),
    );
    render_action_button(
        frame,
        buttons[1],
        "Cancel",
        false,
        false,
        app,
        Some(Message::DismissDialog),
    );

    let mut hint = Label::default()
        .text("Enter: confirm delete  Esc: cancel")
        .alignment(Alignment::Center)
        .foreground(theme.base.text_muted)
        .background(surface);
    hint.view(frame, layout[3]);
}

fn render_rename_repo_dialog(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &mut App,
    state: &RenameRepoDialogState,
) {
    let theme = app.theme;
    let surface = dialog_surface(theme);

    let mut panel =
        dialog_panel("Rename Repo", Alignment::Center, theme, surface).text([TextSpan::from("")]);
    panel.view(frame, area);

    let panel_inner = inset_rect(area, 1, 1);
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(2),
            Constraint::Length(3),
            Constraint::Length(2),
            Constraint::Min(0),
        ])
        .split(panel_inner);

    render_input_component(
        frame,
        layout[0],
        "Display Name",
        &state.name_input,
        matches!(state.focused_field, RenameRepoField::Name),
        theme,
        None,
    );
    app.interaction_map.register_click(
        InteractionLayer::Dialog,
        layout[0],
        Message::FocusRenameRepoField(RenameRepoField::Name),
    );

    let buttons = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(layout[2]);

    render_action_button(
        frame,
        buttons[0],
        "Rename",
        matches!(state.focused_field, RenameRepoField::Confirm),
        false,
        app,
        Some(Message::ConfirmRenameRepo),
    );
    render_action_button(
        frame,
        buttons[1],
        "Cancel",
        matches!(state.focused_field, RenameRepoField::Cancel),
        false,
        app,
        Some(Message::DismissDialog),
    );

    let mut hint = Label::default()
        .text("Tab: next field  Enter: confirm  Esc: cancel")
        .alignment(Alignment::Center)
        .foreground(theme.base.text_muted)
        .background(surface);
    hint.view(frame, layout[3]);

    if matches!(state.focused_field, RenameRepoField::Name) {
        set_text_input_cursor(frame, layout[0], &state.name_input);
    }
}

fn render_delete_repo_dialog(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &mut App,
    state: &DeleteRepoDialogState,
) {
    let theme = app.theme;
    let surface = dialog_surface(theme);

    let mut panel =
        dialog_panel("Remove Repo", Alignment::Center, theme, surface).text([TextSpan::from("")]);
    panel.view(frame, area);

    let panel_inner = inset_rect(area, 1, 1);
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),
            Constraint::Min(1),
            Constraint::Length(3),
            Constraint::Length(2),
        ])
        .split(panel_inner);

    let mut summary = Paragraph::default()
        .foreground(theme.base.text)
        .background(surface)
        .wrap(true)
        .alignment(Alignment::Center)
        .text([TextSpan::from(format!(
            "Remove repo '{}' from this project?\n(Only allowed if no tasks reference it.)",
            state.repo_name
        ))]);
    summary.view(frame, layout[0]);

    let buttons = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(layout[2]);

    render_action_button(
        frame,
        buttons[0],
        "Remove",
        true,
        true,
        app,
        Some(Message::ConfirmDeleteRepo),
    );
    render_action_button(
        frame,
        buttons[1],
        "Cancel",
        false,
        false,
        app,
        Some(Message::DismissDialog),
    );

    let mut hint = Label::default()
        .text("Enter: confirm  Esc: cancel")
        .alignment(Alignment::Center)
        .foreground(theme.base.text_muted)
        .background(surface);
    hint.view(frame, layout[3]);
}

fn render_category_color_dialog(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &mut App,
    state: &crate::app::CategoryColorDialogState,
) {
    let theme = app.theme;
    let surface = dialog_surface(theme);

    let mut panel = dialog_panel("Category Color", Alignment::Center, theme, surface)
        .text([TextSpan::from("")]);
    panel.view(frame, area);

    let panel_inner = inset_rect(area, 1, 1);
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Length(10),
            Constraint::Length(3),
            Constraint::Length(2),
            Constraint::Min(0),
        ])
        .split(panel_inner);

    let mut summary = Paragraph::default()
        .foreground(theme.base.text)
        .background(surface)
        .text([TextSpan::from(format!(
            "Choose color for '{}'",
            state.category_name
        ))]);
    summary.view(frame, layout[0]);

    let mut rows = TableBuilder::default();
    for color in CATEGORY_COLOR_PALETTE {
        rows.add_col(TextSpan::from(category_color_label(color)))
            .add_row();
    }

    let mut palette = List::default()
        .title("Palette", Alignment::Left)
        .borders(rounded_borders(dialog_input_border(
            theme,
            matches!(state.focused_field, CategoryColorField::Palette),
        )))
        .foreground(theme.base.text)
        .highlighted_color(theme.interactive.focus)
        .rows(rows.build())
        .selected_line(
            state
                .selected_index
                .min(CATEGORY_COLOR_PALETTE.len().saturating_sub(1)),
        );
    palette.attr(
        Attribute::Focus,
        AttrValue::Flag(matches!(state.focused_field, CategoryColorField::Palette)),
    );
    palette.view(frame, layout[1]);

    let buttons = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(layout[2]);

    render_action_button(
        frame,
        buttons[0],
        "Save",
        matches!(state.focused_field, CategoryColorField::Confirm),
        false,
        app,
        Some(Message::ConfirmCategoryColor),
    );
    render_action_button(
        frame,
        buttons[1],
        "Cancel",
        matches!(state.focused_field, CategoryColorField::Cancel),
        false,
        app,
        Some(Message::DismissDialog),
    );

    let mut hint = Label::default()
        .text("Arrows/jk: navigate  Tab: next field  Enter: confirm  Esc: cancel")
        .alignment(Alignment::Center)
        .foreground(theme.base.text_muted)
        .background(surface);
    hint.view(frame, layout[3]);
}

fn render_message_dialog(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &mut App,
    title: &str,
    text: &str,
) {
    let theme = app.theme;
    let mut paragraph = dialog_panel(title, Alignment::Center, theme, dialog_surface(theme))
        .wrap(true)
        .text(text.lines().map(|line| TextSpan::from(line.to_string())));
    paragraph.view(frame, area);

    app.interaction_map
        .register_click(InteractionLayer::Dialog, area, Message::DismissDialog);
}

fn render_error_dialog(frame: &mut Frame<'_>, area: Rect, app: &mut App, title: &str, text: &str) {
    let theme = app.theme;
    let surface = dialog_surface(theme);
    let mut paragraph = Paragraph::default()
        .title(title, Alignment::Center)
        .borders(rounded_borders(theme.base.danger))
        .foreground(theme.base.danger)
        .background(surface)
        .wrap(true)
        .text(text.lines().map(|line| TextSpan::from(line.to_string())));
    paragraph.view(frame, area);

    app.interaction_map
        .register_click(InteractionLayer::Dialog, area, Message::DismissDialog);
}

fn render_action_button(
    frame: &mut Frame<'_>,
    area: Rect,
    label: &str,
    focused: bool,
    destructive: bool,
    app: &mut App,
    click_message: Option<Message>,
) {
    let theme = app.theme;
    let hovered = click_message
        .as_ref()
        .is_some_and(|msg| app.hovered_message.as_ref() == Some(msg));
    let highlighted = focused || hovered;
    let (accent, fg, bg) = dialog_button_palette(theme, highlighted, destructive);

    let mut button = Paragraph::default()
        .borders(rounded_borders(accent))
        .foreground(fg)
        .background(if highlighted {
            bg
        } else {
            dialog_surface(theme)
        })
        .alignment(Alignment::Center)
        .text([TextSpan::from(label.to_string())]);
    button.view(frame, area);

    if let Some(message) = click_message {
        app.interaction_map
            .register_click(InteractionLayer::Dialog, area, message);
    }
}

fn render_command_palette_dialog(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &mut App,
    state: &crate::command_palette::CommandPaletteState,
) {
    let theme = app.theme;
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(1),
            Constraint::Min(0),
        ])
        .split(area);

    render_input_component(
        frame,
        chunks[0],
        "Command Palette",
        &state.query,
        true,
        theme,
        None,
    );
    set_text_input_cursor(frame, chunks[0], &state.query);

    let mut hint = Label::default()
        .text("Type to filter. Enter to execute. Esc to close.")
        .alignment(Alignment::Left)
        .foreground(theme.base.text_muted)
        .background(dialog_surface(theme));
    hint.view(frame, chunks[1]);

    if !should_render_command_palette_results(app.viewport) {
        return;
    }

    let mut rows = TableBuilder::default();
    let commands = all_commands();
    for ranked in &state.filtered {
        if let Some(command) = commands.get(ranked.command_idx) {
            let keybinding = app
                .keybindings
                .command_palette_keybinding(command.id)
                .unwrap_or_else(|| command.keybinding.to_string());
            rows.add_col(TextSpan::from(command.display_name.to_string()))
                .add_col(TextSpan::from(keybinding))
                .add_row();
        }
    }

    if state.filtered.is_empty() {
        rows.add_col(TextSpan::from("No matching commands"))
            .add_col(TextSpan::from(""))
            .add_row();
    }

    let selected = state
        .selected_index
        .min(state.filtered.len().saturating_sub(1));
    let mut list = Table::default()
        .title("Results", Alignment::Left)
        .borders(dialog_border(theme))
        .foreground(theme.base.text)
        .background(dialog_surface(theme))
        .highlighted_color(theme.interactive.focus)
        .highlighted_str("> ")
        .headers(["Command", "Key"])
        .widths(&[75, 25])
        .scroll(true)
        .table(rows.build())
        .selected_line(selected)
        .inactive(Style::default().fg(theme.base.text_muted));
    list.attr(Attribute::Focus, AttrValue::Flag(true));
    list.view(frame, chunks[2]);

    let results_area = chunks[2];
    let content_x = results_area.x.saturating_add(1);
    let content_y = results_area.y.saturating_add(1);
    let content_width = results_area.width.saturating_sub(2);
    let max_rows = results_area.height.saturating_sub(2) as usize;
    for idx in 0..state.filtered.len().min(max_rows) {
        app.interaction_map.register_click(
            InteractionLayer::Dialog,
            Rect::new(content_x, content_y + idx as u16, content_width, 1),
            Message::SelectCommandPaletteItem(idx),
        );
    }
}

fn render_help_overlay(frame: &mut Frame<'_>, app: &App) {
    let area = centered_rect(84, 84, frame.area());
    frame.render_widget(Clear, area);
    let theme = app.theme;
    let lines = app
        .keybindings
        .help_lines()
        .into_iter()
        .map(|line| {
            if line.is_empty() {
                TextSpan::new(line)
            } else if !line.starts_with(' ') {
                TextSpan::new(line).fg(theme.base.header).bold()
            } else {
                TextSpan::new(line).fg(theme.base.text)
            }
        })
        .collect::<Vec<_>>();

    let mut paragraph = dialog_panel("Help", Alignment::Center, theme, dialog_surface(theme))
        .wrap(true)
        .text(lines);
    paragraph.view(frame, area);
}

fn render_log_expanded_overlay(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let theme = app.theme;
    let overlay = centered_rect(94, 94, area);
    frame.render_widget(Clear, overlay);

    let entries = app
        .current_log_buffer
        .as_deref()
        .map(parse_structured_log_entries)
        .unwrap_or_default();
    let selected_entry = if entries.is_empty() {
        0
    } else {
        app.log_expanded_scroll_offset.min(entries.len() - 1)
    };
    let viewport = list_inner_height(overlay);

    let visible_lines = if entries.is_empty() {
        vec![TextSpan::new("No log output available.").fg(theme.base.text_muted)]
    } else {
        log_entries_to_spans(
            &entries,
            theme,
            &app.log_expanded_entries,
            selected_entry,
            viewport,
        )
    };

    let mut paragraph = Paragraph::default()
        .title(
            "Logs | structured  j/k select  Ctrl+u/d half-page  gg/G top/bottom  e/Enter toggle  Esc/f close",
            Alignment::Left,
        )
        .borders(rounded_borders(theme.interactive.focus))
        .foreground(theme.base.text)
        .background(dialog_surface(theme))
        .text(visible_lines);
    paragraph.view(frame, overlay);

    let entry_count = entries.len().max(1);
    let viewport_entries = viewport.min(entry_count).max(1);
    if viewport > 0 && entry_count > viewport_entries {
        let mut state = ScrollbarState::new(entry_count)
            .position(scrollbar_position_for_offset(
                selected_entry,
                entry_count,
                viewport_entries,
            ))
            .viewport_content_length(viewport_entries);
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(None)
            .end_symbol(None)
            .track_symbol(Some("│"))
            .track_style(RatatuiStyle::default().fg(theme.base.text_muted))
            .thumb_style(RatatuiStyle::default().fg(theme.interactive.focus))
            .thumb_symbol("█");
        let scrollbar_area = inset_rect(overlay, 1, 1);
        if scrollbar_area.width > 0 && scrollbar_area.height > 0 {
            frame.render_stateful_widget(scrollbar, scrollbar_area, &mut state);
        }
    }
}

fn render_empty_state(frame: &mut Frame<'_>, area: Rect, message: &str, app: &App) {
    let theme = app.theme;
    let mut paragraph = Paragraph::default()
        .title("opencode-kanban", Alignment::Center)
        .borders(rounded_borders(theme.base.text_muted))
        .foreground(theme.base.text_muted)
        .wrap(true)
        .text([TextSpan::from(message.to_string())]);
    paragraph.view(frame, area);
}

fn render_input_component(
    frame: &mut Frame<'_>,
    area: Rect,
    title: &str,
    value: &str,
    focused: bool,
    theme: Theme,
    placeholder: Option<&str>,
) {
    let (display_value, using_placeholder) = resolve_input_display_value(value, placeholder);
    let text_color = if using_placeholder {
        theme.base.text_muted
    } else {
        theme.base.text
    };

    let mut input = Input::default()
        .title(title, Alignment::Left)
        .borders(rounded_borders(dialog_input_border(theme, focused)))
        .foreground(text_color)
        .background(dialog_surface(theme))
        .inactive(Style::default().fg(theme.base.text_muted))
        .input_type(InputType::Text)
        .value(display_value.to_string());
    input.attr(Attribute::Focus, AttrValue::Flag(focused));
    input.view(frame, area);
}

fn resolve_input_display_value<'a>(
    value: &'a str,
    placeholder: Option<&'a str>,
) -> (&'a str, bool) {
    if value.is_empty()
        && let Some(placeholder_text) = placeholder
    {
        return (placeholder_text, true);
    }
    (value, false)
}

fn set_text_input_cursor(frame: &mut Frame<'_>, area: Rect, value: &str) {
    if let Some((cursor_x, cursor_y)) = text_input_cursor_position(area, value) {
        frame.set_cursor_position((cursor_x, cursor_y));
    }
}

fn text_input_cursor_position(area: Rect, value: &str) -> Option<(u16, u16)> {
    if area.width <= 2 || area.height <= 2 {
        return None;
    }

    let content_width = area.width.saturating_sub(2) as usize;
    if content_width == 0 {
        return None;
    }

    let x_offset = value.chars().count().min(content_width.saturating_sub(1));
    let cursor_x = area.x.saturating_add(1).saturating_add(x_offset as u16);
    let cursor_y = area.y.saturating_add(1);
    Some((cursor_x, cursor_y))
}

#[allow(clippy::too_many_arguments)]
fn render_repo_picker_input_component(
    frame: &mut Frame<'_>,
    area: Rect,
    title: &str,
    value: &str,
    editing: bool,
    focused: bool,
    background: Color,
    theme: Theme,
) {
    let suffix = if editing {
        " [Selecting...]"
    } else {
        " [Enter to select]"
    };
    let title = format!("{title}{suffix}");

    let mut input = Input::default()
        .title(&title, Alignment::Left)
        .borders(rounded_borders(dialog_input_border(
            theme,
            focused || editing,
        )))
        .foreground(theme.base.text)
        .background(background)
        .inactive(Style::default().fg(theme.base.text_muted))
        .input_type(InputType::Text)
        .value(value.to_string());
    input.attr(Attribute::Focus, AttrValue::Flag(focused || editing));
    input.view(frame, area);
}

fn render_repo_picker_dialog(
    frame: &mut Frame<'_>,
    app: &App,
    picker: &crate::app::RepoPickerDialogState,
) {
    let theme = app.theme;
    let overlay = calculate_overlay_area(OverlayAnchor::Top, 88, 62, frame.area());
    frame.render_widget(Clear, overlay);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(1),
            Constraint::Min(0),
        ])
        .split(overlay);

    let (picker_title, picker_hint) = match picker.target {
        RepoPickerTarget::Repo => (
            "Select Repository Folder",
            "Type path/name. Up/Down move, Enter accept, Tab complete, Esc close",
        ),
        RepoPickerTarget::ExistingDirectory => (
            "Select Existing Directory",
            "Type path. Up/Down move, Enter accept, Tab complete, Esc close",
        ),
    };

    render_input_component(
        frame,
        chunks[0],
        picker_title,
        &picker.query,
        true,
        theme,
        None,
    );
    set_text_input_cursor(frame, chunks[0], &picker.query);

    let mut hint = Label::default()
        .text(picker_hint)
        .alignment(Alignment::Left)
        .foreground(theme.base.text_muted)
        .background(dialog_surface(theme));
    hint.view(frame, chunks[1]);

    let mut rows = TableBuilder::default();
    for suggestion in &picker.suggestions {
        let kind = match suggestion.kind {
            crate::app::RepoSuggestionKind::KnownRepo { .. } => "Repo",
            crate::app::RepoSuggestionKind::FolderPath => "Folder",
        };
        rows.add_col(TextSpan::from(kind.to_string()))
            .add_col(TextSpan::from(suggestion.label.clone()))
            .add_col(TextSpan::from(suggestion.value.clone()))
            .add_row();
    }

    if picker.suggestions.is_empty() {
        rows.add_col(TextSpan::from(""))
            .add_col(TextSpan::from("No matching folders or repositories"))
            .add_col(TextSpan::from(""))
            .add_row();
    }

    let selected = picker
        .selected_index
        .min(picker.suggestions.len().saturating_sub(1));
    let mut table = Table::default()
        .title("Suggestions", Alignment::Left)
        .borders(dialog_border(theme))
        .foreground(theme.base.text)
        .background(dialog_surface(theme))
        .highlighted_color(theme.interactive.focus)
        .highlighted_str("> ")
        .headers(["Type", "Name", "Path"])
        .widths(&[12, 24, 64])
        .scroll(true)
        .table(rows.build())
        .selected_line(selected)
        .inactive(Style::default().fg(theme.base.text_muted));
    table.attr(Attribute::Focus, AttrValue::Flag(true));
    table.view(frame, chunks[2]);
}

fn tasks_for_category(app: &App, category_id: uuid::Uuid) -> Vec<Task> {
    let mut tasks: Vec<Task> = app
        .tasks
        .iter()
        .filter(|task| task.category_id == category_id)
        .cloned()
        .collect();
    tasks.sort_by_key(|task| task.position);
    tasks
}

fn sorted_categories(app: &App) -> Vec<(usize, Category)> {
    let mut categories: Vec<(usize, Category)> =
        app.categories.iter().cloned().enumerate().collect();
    categories.sort_by_key(|(_, category)| category.position);
    categories
}

fn effective_scroll_column_width(configured_width: usize, viewport_width: usize) -> usize {
    if viewport_width == 0 {
        return 0;
    }

    let fallback_min_width = 16usize;
    let max_visible_width = viewport_width.saturating_sub(4).max(fallback_min_width);
    configured_width.min(max_visible_width).max(1)
}

fn scroll_strip_width(column_count: usize, column_width: usize, gap: usize) -> usize {
    column_width
        .saturating_mul(column_count)
        .saturating_add(gap.saturating_mul(column_count.saturating_sub(1)))
}

const FOCUS_PEEK_CHARS: usize = 6;

fn focused_viewport_offset(
    current_offset: usize,
    viewport_width: usize,
    column_width: usize,
    gap: usize,
    column_count: usize,
    focused_slot: usize,
) -> usize {
    if viewport_width == 0 || column_count == 0 || column_width == 0 {
        return 0;
    }

    let focused_slot = focused_slot.min(column_count.saturating_sub(1));
    let stride = column_width.saturating_add(gap);
    let strip_width = scroll_strip_width(column_count, column_width, gap);
    let focused_left = focused_slot.saturating_mul(stride);
    let focused_right = focused_left.saturating_add(column_width);

    let available_peek = viewport_width.saturating_sub(column_width);
    let mut left_peek = if focused_slot > 0 {
        FOCUS_PEEK_CHARS.min(available_peek)
    } else {
        0
    };
    let mut right_peek = if focused_slot + 1 < column_count {
        FOCUS_PEEK_CHARS.min(available_peek)
    } else {
        0
    };
    while left_peek.saturating_add(right_peek) > available_peek {
        if right_peek >= left_peek && right_peek > 0 {
            right_peek -= 1;
        } else if left_peek > 0 {
            left_peek -= 1;
        } else {
            break;
        }
    }

    let desired_left = focused_left.saturating_sub(left_peek);
    let desired_right = focused_right.saturating_add(right_peek);

    let mut viewport_x = current_offset.min(strip_width.saturating_sub(viewport_width));

    if desired_left < viewport_x {
        viewport_x = desired_left;
    } else if desired_right > viewport_x.saturating_add(viewport_width) {
        viewport_x = desired_right.saturating_sub(viewport_width);
    }

    viewport_x.min(strip_width.saturating_sub(viewport_width))
}

fn side_panel_row_lines(row: &SidePanelRow) -> usize {
    match row {
        SidePanelRow::CategoryHeader { .. } => 1,
        SidePanelRow::Task { task, .. } => task_tile_lines(task),
    }
}

fn side_panel_selected_line(rows: &[SidePanelRow], selected_row: usize) -> usize {
    selected_line_for_row_heights(
        &rows.iter().map(side_panel_row_lines).collect::<Vec<_>>(),
        selected_row,
    )
}

fn column_selected_line(tasks: &[Task], selected_task: usize) -> usize {
    selected_line_for_row_heights(
        &tasks.iter().map(task_tile_lines).collect::<Vec<_>>(),
        selected_task,
    )
}

fn column_scroll_offset(selected_line: usize, row_count: usize, viewport_lines: usize) -> usize {
    if viewport_lines == 0 {
        return 0;
    }
    let max_offset = row_count.saturating_sub(viewport_lines);
    selected_line
        .saturating_sub(viewport_lines.saturating_sub(1))
        .min(max_offset)
}

fn selected_line_for_row_heights(row_heights: &[usize], selected_index: usize) -> usize {
    if row_heights.is_empty() {
        return 0;
    }

    let selected_index = selected_index.min(row_heights.len() - 1);
    let selected_start = row_heights.iter().take(selected_index).sum::<usize>();
    let selected_height = row_heights.get(selected_index).copied().unwrap_or(1);
    let row_count = row_heights.iter().sum::<usize>();

    selected_start
        .saturating_add(selected_height.saturating_sub(1))
        .min(row_count.saturating_sub(1))
}

fn scrollbar_position_for_offset(
    scroll_offset: usize,
    row_count: usize,
    viewport_lines: usize,
) -> usize {
    if row_count == 0 || viewport_lines == 0 {
        return 0;
    }

    let max_offset = row_count.saturating_sub(viewport_lines);
    if max_offset == 0 {
        return 0;
    }

    let max_position = row_count.saturating_sub(1);
    let clamped_offset = scroll_offset.min(max_offset);
    ((clamped_offset as u128) * (max_position as u128) / (max_offset as u128)) as usize
}

fn category_status_counts(app: &App, category_id: uuid::Uuid) -> (usize, usize) {
    let mut running = 0;
    let mut idle = 0;

    for task in app
        .tasks
        .iter()
        .filter(|task| task.category_id == category_id)
    {
        if task.tmux_status == "running" {
            running += 1;
        } else {
            idle += 1;
        }
    }

    (running, idle)
}

const TASK_TITLE_MAX: usize = 34;
const TASK_REPO_MAX: usize = 18;
const TASK_BRANCH_MAX: usize = 34;

fn task_tile_lines(_task: &Task) -> usize {
    5
}

fn append_task_tile_rows(
    rows: &mut TableBuilder,
    app: &App,
    task: &Task,
    is_selected: bool,
    tile_width: usize,
    selected_border: Color,
) {
    let theme = app.theme;
    let tile = theme.tile_colors(is_selected);
    let bg = tile.background;
    let border = if is_selected {
        selected_border
    } else {
        tile.border
    };
    let inner_width = tile_width.saturating_sub(2).max(4);

    let top = format!("┌{}┐", "─".repeat(inner_width));
    rows.add_col(TextSpan::new(top).fg(border).bg(bg)).add_row();

    let status_line = pad_to_width(
        &format!(" {}", task_tile_status_line(app, task)),
        inner_width,
    );
    rows.add_col(TextSpan::new("│").fg(border).bg(bg))
        .add_col(
            TextSpan::new(status_line)
                .fg(theme.status_color(task.tmux_status.as_str()))
                .bg(bg)
                .bold(),
        )
        .add_col(TextSpan::new("│").fg(border).bg(bg))
        .add_row();

    let title_line = pad_to_width(&format!(" {}", task_tile_title(task)), inner_width);
    rows.add_col(TextSpan::new("│").fg(border).bg(bg))
        .add_col(TextSpan::new(title_line).fg(theme.base.text).bg(bg).bold())
        .add_col(TextSpan::new("│").fg(border).bg(bg))
        .add_row();

    let repo = task_tile_repo(app, task);
    let branch = task_tile_branch(task);
    let used = 1 + count_chars(&repo) + 1 + count_chars(&branch);
    let filler = inner_width.saturating_sub(used);

    rows.add_col(TextSpan::new("│").fg(border).bg(bg))
        .add_col(TextSpan::new(" ").bg(bg))
        .add_col(TextSpan::new(repo).fg(theme.tile.repo).bg(bg))
        .add_col(TextSpan::new(":").fg(theme.base.text_muted).bg(bg))
        .add_col(TextSpan::new(branch).fg(theme.tile.branch).bg(bg))
        .add_col(TextSpan::new(" ".repeat(filler)).bg(bg))
        .add_col(TextSpan::new("│").fg(border).bg(bg))
        .add_row();

    let bottom = format!("└{}┘", "─".repeat(inner_width));
    rows.add_col(TextSpan::new(bottom).fg(border).bg(bg))
        .add_row();
}

fn task_tile_status_line(app: &App, task: &Task) -> String {
    let spinner = status_spinner_ascii(task.tmux_status.as_str(), app.pulse_phase);
    match app.session_todo_summary(task.id) {
        Some((done, total)) => format!("{spinner}  todo {done}/{total}"),
        None => spinner.to_string(),
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum TodoLineState {
    Completed,
    Active,
    Pending,
}

fn todo_checklist_lines(todos: &[SessionTodoItem]) -> Vec<(String, TodoLineState)> {
    let active_index = todos.iter().position(|todo| !todo.completed);

    todos
        .iter()
        .enumerate()
        .map(|(index, todo)| {
            let state = if todo.completed {
                TodoLineState::Completed
            } else if Some(index) == active_index {
                TodoLineState::Active
            } else {
                TodoLineState::Pending
            };
            let marker = todo_line_marker(state);
            let content = clamp_text(todo.content.as_str(), 72);
            (format!("┃  [{marker}] {content}"), state)
        })
        .collect()
}

fn todo_line_marker(state: TodoLineState) -> &'static str {
    match state {
        TodoLineState::Completed => "✓",
        TodoLineState::Active => "•",
        TodoLineState::Pending => " ",
    }
}

fn todo_state_color(theme: Theme, state: TodoLineState) -> Color {
    match state {
        TodoLineState::Completed => theme.status.running,
        TodoLineState::Active => theme.status.waiting,
        TodoLineState::Pending => theme.base.text_muted,
    }
}

fn subagent_count_color(theme: Theme, todo_summary: Option<(usize, usize)>) -> Color {
    match todo_summary {
        Some((done, total)) if total > 0 && done >= total => theme.status.running,
        Some((_, total)) if total > 0 => theme.tile.todo,
        _ => theme.base.text_muted,
    }
}

fn task_tile_title(task: &Task) -> String {
    clamp_text(task.title.as_str(), TASK_TITLE_MAX)
}

fn task_tile_repo(app: &App, task: &Task) -> String {
    let repo = app
        .repos
        .iter()
        .find(|repo| repo.id == task.repo_id)
        .map(|repo| repo.name.as_str())
        .unwrap_or("unknown");
    clamp_text(repo, TASK_REPO_MAX)
}

fn task_tile_branch(task: &Task) -> String {
    clamp_text(task.branch.as_str(), TASK_BRANCH_MAX)
}

fn list_inner_width(area: Rect) -> usize {
    area.width.saturating_sub(2) as usize
}

fn list_inner_height(area: Rect) -> usize {
    area.height.saturating_sub(2) as usize
}

fn count_chars(value: &str) -> usize {
    value.chars().count()
}

fn pad_to_width(value: &str, width: usize) -> String {
    let len = count_chars(value);
    if len >= width {
        return clamp_text(value, width);
    }
    format!("{}{}", value, " ".repeat(width - len))
}

fn detail_kv(label: &str, value: &str) -> String {
    format!("{label:>8}: {value}")
}

#[derive(Debug, Clone)]
struct StructuredLogEntry {
    header: String,
    details: Vec<String>,
}

fn parse_structured_log_entries(log: &str) -> Vec<StructuredLogEntry> {
    let mut entries = Vec::new();
    let mut current: Option<StructuredLogEntry> = None;

    for line in log.lines() {
        if line.starts_with("> [") {
            if let Some(entry) = current.take() {
                entries.push(entry);
            }
            current = Some(StructuredLogEntry {
                header: line.to_string(),
                details: Vec::new(),
            });
            continue;
        }

        if line.trim().is_empty() {
            continue;
        }

        let detail = line.trim_start().to_string();
        if let Some(entry) = current.as_mut() {
            entry.details.push(detail);
        } else {
            current = Some(StructuredLogEntry {
                header: line.to_string(),
                details: Vec::new(),
            });
        }
    }

    if let Some(entry) = current {
        entries.push(entry);
    }

    entries
}

fn log_entries_to_spans(
    entries: &[StructuredLogEntry],
    theme: Theme,
    expanded_entries: &HashSet<usize>,
    selected_entry: usize,
    viewport: usize,
) -> Vec<TextSpan> {
    let mut lines = Vec::new();
    let mut index = selected_entry.saturating_sub(2);

    while index < entries.len() {
        if viewport > 0 && lines.len() >= viewport {
            break;
        }

        let entry = &entries[index];
        let is_selected = index == selected_entry;
        let is_expanded = expanded_entries.contains(&index);
        let caret = if is_expanded { "▾" } else { "▸" };
        let selector = if is_selected { "▶" } else { " " };
        let header_body = entry
            .header
            .strip_prefix("> ")
            .unwrap_or(entry.header.as_str());
        let header_line = format!("{selector}{caret} {header_body}");
        let header_kind = parse_structured_log_kind(entry.header.as_str()).unwrap_or("TEXT");
        let header_color = if is_selected {
            theme.interactive.focus
        } else {
            structured_log_kind_color(theme, header_kind)
        };

        lines.push(TextSpan::new(header_line).fg(header_color).bold());

        if is_expanded {
            if entry.details.is_empty() {
                if viewport == 0 || lines.len() < viewport {
                    lines.push(TextSpan::new("    (no content)").fg(theme.base.text_muted));
                }
            } else {
                for detail in &entry.details {
                    if viewport > 0 && lines.len() >= viewport {
                        break;
                    }
                    lines.push(TextSpan::new(format!("    {detail}")).fg(theme.base.text));
                }
            }
        }

        index += 1;
    }

    lines
}

fn parse_structured_log_kind(line: &str) -> Option<&str> {
    let rest = line.strip_prefix("> [")?;
    let (kind, _) = rest.split_once(']')?;
    Some(kind)
}

fn structured_log_kind_color(theme: Theme, kind: &str) -> Color {
    if kind.eq_ignore_ascii_case("SAY") {
        theme.base.header
    } else if kind.eq_ignore_ascii_case("TOOL") {
        theme.tile.repo
    } else if kind.eq_ignore_ascii_case("THINK") {
        theme.tile.branch
    } else if kind.eq_ignore_ascii_case("STEP+") || kind.eq_ignore_ascii_case("STEP-") {
        theme.tile.todo
    } else if kind.eq_ignore_ascii_case("RETRY") {
        theme.status.dead
    } else if kind.eq_ignore_ascii_case("PATCH") {
        theme.interactive.focus
    } else {
        theme.base.header
    }
}

fn format_archive_time(iso_timestamp: &str) -> String {
    if let Ok(dt) = DateTime::parse_from_rfc3339(iso_timestamp) {
        let local = dt.with_timezone(&Utc);
        local.format("%Y-%m-%d %H:%M").to_string()
    } else {
        iso_timestamp.to_string()
    }
}

fn clamp_text(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_string();
    }
    if max_chars <= 3 {
        return "...".to_string();
    }
    let mut shortened = value.chars().take(max_chars - 3).collect::<String>();
    shortened.push_str("...");
    shortened
}

fn status_spinner_ascii(status: &str, pulse_phase: u8) -> &'static str {
    match status {
        "running" => match pulse_phase % 4 {
            0 => ".:",
            1 => "::",
            2 => ":.",
            _ => "..",
        },
        _ => "--",
    }
}

fn rounded_borders(color: Color) -> Borders {
    Borders::default()
        .modifiers(BorderType::Rounded)
        .color(color)
}

fn dialog_surface(theme: Theme) -> Color {
    theme.dialog_surface()
}

fn dialog_border(theme: Theme) -> Borders {
    rounded_borders(theme.interactive.focus)
}

fn dialog_panel(title: &str, alignment: Alignment, theme: Theme, background: Color) -> Paragraph {
    Paragraph::default()
        .title(title, alignment)
        .borders(dialog_border(theme))
        .foreground(theme.base.text)
        .background(background)
}

fn dialog_checkbox(title: &str, theme: Theme, background: Color) -> Checkbox {
    Checkbox::default()
        .title(title, Alignment::Left)
        .borders(dialog_border(theme))
        .foreground(theme.base.text)
        .background(background)
        .inactive(Style::default().fg(theme.base.text_muted))
}

fn render_mode_segmented_control(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    mode_focused: bool,
    use_existing_directory: bool,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let theme = app.theme;
    let left_width = area.width / 2;
    let right_width = area.width.saturating_sub(left_width);

    let left_selected = !use_existing_directory;
    let right_selected = use_existing_directory;
    let left_hovered =
        app.hovered_message.as_ref() == Some(&Message::SetNewTaskUseExistingDirectory(false));
    let right_hovered =
        app.hovered_message.as_ref() == Some(&Message::SetNewTaskUseExistingDirectory(true));

    let (left_fg, left_bg) = mode_segment_colors(theme, mode_focused, left_selected, left_hovered);
    let (right_fg, right_bg) =
        mode_segment_colors(theme, mode_focused, right_selected, right_hovered);

    let left_label = centered_label("New worktree", left_width);
    let right_label = centered_label("Existing directory", right_width);

    let line = RatatuiLine::from(vec![
        RatatuiSpan::styled(left_label, RatatuiStyle::default().fg(left_fg).bg(left_bg)),
        RatatuiSpan::styled(
            right_label,
            RatatuiStyle::default().fg(right_fg).bg(right_bg),
        ),
    ]);

    let paragraph =
        RatatuiParagraph::new(line).style(RatatuiStyle::default().bg(dialog_surface(theme)));
    frame.render_widget(paragraph, area);
}

fn mode_segment_colors(
    theme: Theme,
    mode_focused: bool,
    selected: bool,
    hovered: bool,
) -> (Color, Color) {
    let bg = if selected {
        theme.interactive.focus
    } else if hovered {
        theme.dialog.button_bg
    } else {
        dialog_surface(theme)
    };
    let fg = if selected {
        theme.dialog.button_fg
    } else if mode_focused || hovered {
        theme.base.text
    } else {
        theme.base.text_muted
    };
    (fg, bg)
}

fn centered_label(label: &str, width: u16) -> String {
    let width = width as usize;
    if width == 0 {
        return String::new();
    }

    let mut text = label.to_string();
    if text.len() > width {
        text.truncate(width);
        return text;
    }

    let padding = width.saturating_sub(text.len());
    let left = padding / 2;
    let right = padding - left;
    format!("{}{}{}", " ".repeat(left), text, " ".repeat(right))
}

fn delete_task_checkbox_focus_index(field: DeleteTaskField) -> Option<usize> {
    match field {
        DeleteTaskField::KillTmux => Some(0),
        DeleteTaskField::RemoveWorktree => Some(1),
        DeleteTaskField::DeleteBranch => Some(2),
        DeleteTaskField::Delete | DeleteTaskField::Cancel => None,
    }
}

fn set_checkbox_highlight_choice(checkbox: &mut Checkbox, choice: Option<usize>) {
    if let Some(choice) = choice {
        for _ in 0..choice {
            let _ = checkbox.perform(Cmd::Move(CmdDirection::Right));
        }
    }
}

fn dialog_button_palette(theme: Theme, focused: bool, destructive: bool) -> (Color, Color, Color) {
    let accent = if destructive {
        theme.base.danger
    } else {
        theme.interactive.focus
    };
    let fg = if focused {
        theme.dialog.button_fg
    } else {
        accent
    };
    let bg = if focused {
        accent
    } else {
        theme.dialog.button_bg
    };
    (accent, fg, bg)
}

fn dialog_input_border(theme: Theme, focused: bool) -> Color {
    if focused {
        theme.interactive.focus
    } else {
        theme.base.text_muted
    }
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

fn render_context_menu(frame: &mut Frame<'_>, app: &mut App) {
    let Some(ref menu) = app.context_menu else {
        return;
    };

    let theme = app.theme;
    let items = &menu.items;
    let item_labels: Vec<&str> = items
        .iter()
        .map(|item| match item {
            ContextMenuItem::Attach => " Attach ",
            ContextMenuItem::Edit => " Edit   ",
            ContextMenuItem::Delete => " Delete ",
            ContextMenuItem::Move => " Move   ",
        })
        .collect();

    let width = item_labels.iter().map(|s| s.len()).max().unwrap_or(8) as u16 + 2;
    let height = items.len() as u16 + 2;

    let (mx, my) = menu.position;
    let frame_width = frame.area().width;
    let frame_height = frame.area().height;

    let x = mx.min(frame_width.saturating_sub(width));
    let y = my.min(frame_height.saturating_sub(height));

    let menu_rect = Rect::new(x, y, width, height);
    frame.render_widget(Clear, menu_rect);

    let mut rows = TableBuilder::default();
    for (idx, label) in item_labels.iter().enumerate() {
        if idx == menu.selected_index {
            rows.add_col(
                TextSpan::new(*label)
                    .fg(theme.base.canvas)
                    .bg(theme.interactive.focus)
                    .bold(),
            );
        } else {
            rows.add_col(
                TextSpan::new(*label)
                    .fg(theme.base.text)
                    .bg(theme.base.surface),
            );
        }
        rows.add_row();
    }

    let mut list = List::default()
        .borders(rounded_borders(theme.interactive.focus))
        .foreground(theme.base.text)
        .background(theme.base.surface)
        .rows(rows.build());
    list.view(frame, menu_rect);

    for (idx, item) in items.iter().enumerate() {
        let item_y = y + 1 + idx as u16;
        let item_rect = Rect::new(x + 1, item_y, width.saturating_sub(2), 1);
        let msg = match item {
            ContextMenuItem::Attach => Message::AttachSelectedTask,
            ContextMenuItem::Edit => Message::OpenEditTaskDialog,
            ContextMenuItem::Delete => Message::OpenDeleteTaskDialog,
            ContextMenuItem::Move => Message::DismissDialog,
        };
        app.interaction_map
            .register_click(InteractionLayer::ContextMenu, item_rect, msg);
    }
}

fn command_palette_overlay_size(viewport: (u16, u16)) -> (u16, u16) {
    if viewport.0 < 30 { (90, 50) } else { (60, 50) }
}

fn should_render_command_palette_results(viewport: (u16, u16)) -> bool {
    viewport.1 >= 10
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

fn inset_rect(area: Rect, horizontal: u16, vertical: u16) -> Rect {
    let x = area.x.saturating_add(horizontal);
    let y = area.y.saturating_add(vertical);
    let width = area.width.saturating_sub(horizontal.saturating_mul(2));
    let height = area.height.saturating_sub(vertical.saturating_mul(2));
    Rect::new(x, y, width, height)
}

fn render_settings(frame: &mut Frame<'_>, app: &mut App) {
    let theme = app.theme;
    let mut canvas = Paragraph::default()
        .background(theme.base.surface)
        .text([TextSpan::from("")]);
    canvas.view(frame, frame.area());

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(0),
            Constraint::Length(2),
        ])
        .split(frame.area());

    render_header(frame, chunks[0], app);
    render_settings_content(frame, chunks[1], app);
    render_settings_footer(frame, chunks[2], app);
}

fn render_settings_content(frame: &mut Frame<'_>, area: Rect, app: &mut App) {
    let sections = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(20), Constraint::Percentage(80)])
        .split(area);

    render_settings_sidebar(frame, sections[0], app);
    render_settings_active_section(frame, sections[1], app);
}

fn render_settings_sidebar(frame: &mut Frame<'_>, area: Rect, app: &mut App) {
    let theme = app.theme;
    let active_section = app
        .settings_view_state
        .as_ref()
        .map(|s| s.active_section)
        .unwrap_or(SettingsSection::General);

    let sidebar_sections = [
        SettingsSection::General,
        SettingsSection::CategoryColors,
        SettingsSection::Keybindings,
        SettingsSection::Repos,
    ];

    let mut rows = TableBuilder::default();
    for section in sidebar_sections {
        let label = match section {
            SettingsSection::General => "General",
            SettingsSection::CategoryColors => "Category Colors",
            SettingsSection::Keybindings => "Keybindings",
            SettingsSection::Repos => "Repos",
        };
        let prefix = if section == active_section {
            "> "
        } else {
            "  "
        };
        let is_hovered =
            app.hovered_message.as_ref() == Some(&Message::SettingsSelectSection(section));
        let span = if section == active_section || is_hovered {
            TextSpan::new(format!("{}{}", prefix, label))
                .fg(theme.interactive.focus)
                .bold()
        } else {
            TextSpan::from(format!("{}{}", prefix, label))
        };
        rows.add_col(span).add_row();
    }

    let selected_idx = match active_section {
        SettingsSection::General => 0,
        SettingsSection::CategoryColors => 1,
        SettingsSection::Keybindings => 2,
        SettingsSection::Repos => 3,
    };

    let mut list = List::default()
        .title("Settings", Alignment::Left)
        .borders(rounded_borders(theme.interactive.focus))
        .foreground(theme.base.text)
        .background(theme.base.surface)
        .highlighted_color(theme.interactive.focus)
        .highlighted_str("> ")
        .scroll(false)
        .rows(rows.build())
        .selected_line(selected_idx);
    list.attr(Attribute::Focus, AttrValue::Flag(true));
    list.view(frame, area);

    let content_x = area.x.saturating_add(1);
    let content_y = area.y.saturating_add(1);
    let content_width = area.width.saturating_sub(2);
    for (idx, section) in sidebar_sections.iter().enumerate() {
        let row = content_y.saturating_add(idx as u16);
        if row < area.y.saturating_add(area.height.saturating_sub(1)) {
            app.interaction_map.register_click(
                InteractionLayer::Base,
                Rect::new(content_x, row, content_width, 1),
                Message::SettingsSelectSection(*section),
            );
        }
    }
}

fn render_settings_active_section(frame: &mut Frame<'_>, area: Rect, app: &mut App) {
    let active_section = app
        .settings_view_state
        .as_ref()
        .map(|s| s.active_section)
        .unwrap_or(SettingsSection::General);
    match active_section {
        SettingsSection::General => render_settings_general(frame, area, app),
        SettingsSection::CategoryColors => render_settings_category_colors(frame, area, app),
        SettingsSection::Keybindings => render_settings_keybindings(frame, area, app),
        SettingsSection::Repos => render_settings_repos(frame, area, app),
    }
}
fn render_settings_category_colors(frame: &mut Frame<'_>, area: Rect, app: &mut App) {
    let theme = app.theme;
    let selected_field = app
        .settings_view_state
        .as_ref()
        .map(|s| s.category_color_selected)
        .unwrap_or(0)
        .min(app.categories.len().saturating_sub(1));

    let mut rows = TableBuilder::default();
    if app.categories.is_empty() {
        rows.add_col(TextSpan::from("No categories available"))
            .add_row();
    } else {
        for (index, category) in app.categories.iter().enumerate() {
            let is_hovered =
                app.hovered_message.as_ref() == Some(&Message::SettingsSelectCategoryColor(index));
            let prefix = if index == selected_field || is_hovered {
                "> "
            } else {
                "  "
            };
            let color_label = category_color_label(category.color.as_deref());
            let text = format!("{}{}: {}", prefix, category.name, color_label);
            let span = if is_hovered {
                TextSpan::new(text).fg(theme.interactive.focus).bold()
            } else {
                TextSpan::new(text).fg(theme.category_accent(category.color.as_deref()))
            };
            rows.add_col(span).add_row();
        }
    }

    let mut list = List::default()
        .title("Category Colors", Alignment::Left)
        .borders(rounded_borders(theme.interactive.focus))
        .foreground(theme.base.text)
        .background(theme.base.surface)
        .highlighted_color(theme.interactive.focus)
        .highlighted_str("> ")
        .scroll(false)
        .rows(rows.build())
        .selected_line(selected_field);
    list.attr(Attribute::Focus, AttrValue::Flag(true));
    list.view(frame, area);

    let content_x = area.x.saturating_add(1);
    let content_y = area.y.saturating_add(1);
    let content_width = area.width.saturating_sub(2);
    let max_rows = area.height.saturating_sub(2) as usize;
    for index in 0..app.categories.len().min(max_rows) {
        app.interaction_map.register_click(
            InteractionLayer::Base,
            Rect::new(content_x, content_y + index as u16, content_width, 1),
            Message::SettingsSelectCategoryColor(index),
        );
    }
}

fn render_settings_keybindings(frame: &mut Frame<'_>, area: Rect, app: &mut App) {
    let theme = app.theme;
    let lines = app.keybindings.help_lines();

    let mut rows = TableBuilder::default();
    for line in lines {
        if line.trim().is_empty() {
            rows.add_col(TextSpan::from(" ")).add_row();
        } else if !line.starts_with(' ') {
            rows.add_col(TextSpan::new(line).fg(theme.base.header).bold())
                .add_row();
        } else {
            rows.add_col(TextSpan::new(line).fg(theme.base.text))
                .add_row();
        }
    }

    let mut list = List::default()
        .title("Keybindings (View Only)", Alignment::Left)
        .borders(rounded_borders(theme.interactive.focus))
        .foreground(theme.base.text)
        .background(theme.base.surface)
        .scroll(true)
        .rows(rows.build());
    list.attr(Attribute::Focus, AttrValue::Flag(true));
    list.view(frame, area);
}

fn render_settings_general(frame: &mut Frame<'_>, area: Rect, app: &mut App) {
    let theme = app.theme;
    let selected_field = app
        .settings_view_state
        .as_ref()
        .map(|s| s.general_selected_field)
        .unwrap_or(0);

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(7), Constraint::Min(10)])
        .split(area);

    let field_rows: [(&str, String); 5] = [
        ("Theme", app.settings.theme.clone()),
        (
            "Poll Interval",
            format!("{} ms", app.settings.poll_interval_ms),
        ),
        (
            "Side Panel Width",
            format!("{}%", app.settings.side_panel_width),
        ),
        ("Default View", app.settings.default_view.clone()),
        ("Board Alignment", app.settings.board_alignment_mode.clone()),
    ];

    let mut rows = TableBuilder::default();
    for (i, (label, value)) in field_rows.iter().enumerate() {
        let is_hovered =
            app.hovered_message.as_ref() == Some(&Message::SettingsSelectGeneralField(i));
        let is_selected = i == selected_field || is_hovered;
        let bg = if is_selected {
            theme.interactive.selected_bg
        } else {
            theme.base.surface
        };
        let fg = if is_selected {
            theme.interactive.focus
        } else {
            theme.base.text
        };
        let text = format!("  {:<18} {}", label, value);
        let span = if is_selected {
            TextSpan::new(text).fg(fg).bg(bg).bold()
        } else {
            TextSpan::new(text).fg(fg).bg(bg)
        };
        rows.add_col(span).add_row();
    }

    let mut list = List::default()
        .title(
            "General  (j/k: select  h/l: adjust  0: reset)",
            Alignment::Left,
        )
        .borders(rounded_borders(theme.interactive.focus))
        .foreground(theme.base.text)
        .background(theme.base.surface)
        .scroll(false)
        .rows(rows.build())
        .selected_line(selected_field)
        .inactive(Style::default().fg(theme.base.text_muted));
    list.attr(Attribute::Focus, AttrValue::Flag(true));
    list.view(frame, layout[0]);

    let list_area = layout[0];
    let content_x = list_area.x.saturating_add(1);
    let content_y = list_area.y.saturating_add(1);
    let content_width = list_area.width.saturating_sub(2);
    for index in 0..5 {
        let row = content_y.saturating_add(index as u16);
        if row
            < list_area
                .y
                .saturating_add(list_area.height.saturating_sub(1))
        {
            app.interaction_map.register_click(
                InteractionLayer::Base,
                Rect::new(content_x, row, content_width, 1),
                Message::SettingsSelectGeneralField(index),
            );
        }
    }

    let info_lines: Vec<TextSpan> = match selected_field {
        0 => {
            let current = &app.settings.theme;
            let mut lines = vec![
                TextSpan::new("Theme").fg(theme.base.header).bold(),
                TextSpan::new("Color palette used throughout the app.").fg(theme.base.text),
                TextSpan::new(""),
            ];
            for preset in ThemePreset::ALL {
                let preset_name = preset.as_str();
                let marker = if current == preset_name { "●" } else { "○" };
                lines.push(
                    TextSpan::new(format!(
                        "  {} {:<16}  {}",
                        marker,
                        preset_name,
                        preset.description()
                    ))
                    .fg(if current == preset_name {
                        theme.interactive.focus
                    } else {
                        theme.base.text_muted
                    }),
                );
            }
            lines.push(TextSpan::new(""));
            lines.push(TextSpan::new("  l / →    cycle forward").fg(theme.base.text_muted));
            lines.push(TextSpan::new("  h / ←    cycle backward").fg(theme.base.text_muted));
            lines.push(TextSpan::new("  0         reset to default").fg(theme.base.text_muted));
            lines
        }
        1 => vec![
            TextSpan::new("Poll Interval").fg(theme.base.header).bold(),
            TextSpan::new(
                "How often the app checks each session's status. \
                 Lower values give faster updates but use more CPU.",
            )
            .fg(theme.base.text),
            TextSpan::new(""),
            TextSpan::new(format!("{:>8}: {}", "Range", "500 – 30 000 ms"))
                .fg(theme.base.text_muted),
            TextSpan::new(format!("{:>8}: {}", "Step", "500 ms")).fg(theme.base.text_muted),
            TextSpan::new(format!("{:>8}: {}", "Default", "1 000 ms")).fg(theme.base.text_muted),
            TextSpan::new(""),
            TextSpan::new("  l / →    increase value").fg(theme.base.text_muted),
            TextSpan::new("  h / ←    decrease value").fg(theme.base.text_muted),
            TextSpan::new("  0         reset to default").fg(theme.base.text_muted),
        ],
        2 => vec![
            TextSpan::new("Side Panel Width")
                .fg(theme.base.header)
                .bold(),
            TextSpan::new(
                "Width of the left panel in split view (SidePanel mode). \
                 Adjust to balance task list vs detail pane.",
            )
            .fg(theme.base.text),
            TextSpan::new(""),
            TextSpan::new(format!("{:>8}: {}", "Range", "20 – 80 %")).fg(theme.base.text_muted),
            TextSpan::new(format!("{:>8}: {}", "Step", "5 %")).fg(theme.base.text_muted),
            TextSpan::new(format!("{:>8}: {}", "Default", "40 %")).fg(theme.base.text_muted),
            TextSpan::new(""),
            TextSpan::new("  l / →    increase value").fg(theme.base.text_muted),
            TextSpan::new("  h / ←    decrease value").fg(theme.base.text_muted),
            TextSpan::new("  0         reset to default").fg(theme.base.text_muted),
        ],
        3 => {
            let current = &app.settings.default_view;
            let mut lines = vec![
                TextSpan::new("Default View").fg(theme.base.header).bold(),
                TextSpan::new("View mode shown when opening a project.").fg(theme.base.text),
                TextSpan::new(""),
            ];
            for (view, desc) in [
                ("kanban", "Board with columns per category"),
                ("detail", "Split view with task details"),
            ] {
                let marker = if current == view { "●" } else { "○" };
                lines.push(
                    TextSpan::new(format!("  {} {:<10}  {}", marker, view, desc)).fg(
                        if current == view {
                            theme.interactive.focus
                        } else {
                            theme.base.text_muted
                        },
                    ),
                );
            }
            lines.push(TextSpan::new(""));
            lines.push(TextSpan::new("  l / →    cycle forward").fg(theme.base.text_muted));
            lines.push(TextSpan::new("  h / ←    cycle backward").fg(theme.base.text_muted));
            lines.push(TextSpan::new("  0         reset to default").fg(theme.base.text_muted));
            lines
        }
        4 => {
            let current = &app.settings.board_alignment_mode;
            let mut lines =
                vec![
                TextSpan::new("Board Alignment").fg(theme.base.header).bold(),
                TextSpan::new(
                    "How columns are laid out in Kanban mode. 'fit' keeps the current behavior. \
                     'scroll' keeps fixed-width columns and follows focus horizontally.",
                )
                .fg(theme.base.text),
                TextSpan::new(""),
            ];
            for (mode, desc) in [
                ("fit", "fit all columns into viewport"),
                ("scroll", "fixed-width strip with auto-scroll"),
            ] {
                let marker = if current == mode { "●" } else { "○" };
                lines.push(
                    TextSpan::new(format!("  {} {:<10}  {}", marker, mode, desc)).fg(
                        if current == mode {
                            theme.interactive.focus
                        } else {
                            theme.base.text_muted
                        },
                    ),
                );
            }
            lines.push(TextSpan::new(""));
            lines.push(TextSpan::new("  l / →    cycle forward").fg(theme.base.text_muted));
            lines.push(TextSpan::new("  h / ←    cycle backward").fg(theme.base.text_muted));
            lines.push(TextSpan::new("  0         reset to default").fg(theme.base.text_muted));
            lines
        }
        _ => vec![],
    };

    let mut desc = Paragraph::default()
        .title("Info", Alignment::Left)
        .borders(rounded_borders(theme.base.text_muted))
        .foreground(theme.base.text)
        .background(theme.base.surface)
        .wrap(true)
        .text(info_lines);
    desc.view(frame, layout[1]);
}

fn render_settings_repos(frame: &mut Frame<'_>, area: Rect, app: &mut App) {
    let theme = app.theme;
    let selected_field = app
        .settings_view_state
        .as_ref()
        .map(|s| s.repos_selected_field)
        .unwrap_or(0)
        .min(app.repos.len().saturating_sub(1));

    let mut rows = TableBuilder::default();

    if app.repos.is_empty() {
        rows.add_col(TextSpan::from("  No repos configured for this project"))
            .add_row();
    } else {
        for (index, repo) in app.repos.iter().enumerate() {
            let is_hovered =
                app.hovered_message.as_ref() == Some(&Message::SettingsSelectRepo(index));
            let prefix = if index == selected_field || is_hovered {
                "> "
            } else {
                "  "
            };
            let path_short = clamp_text(&repo.path, 45);
            let base = repo
                .default_base
                .as_deref()
                .filter(|b| !b.is_empty())
                .unwrap_or("—");
            let span = TextSpan::new(format!(
                "{}{:<20} {} [{}]",
                prefix,
                clamp_text(&repo.name, 20),
                path_short,
                base
            ))
            .fg(if is_hovered {
                theme.interactive.focus
            } else {
                theme.base.text
            });
            rows.add_col(span).add_row();
        }
    }

    let mut list = List::default()
        .title("Repositories", Alignment::Left)
        .borders(rounded_borders(theme.interactive.focus))
        .foreground(theme.base.text)
        .highlighted_color(theme.interactive.focus)
        .highlighted_str("> ")
        .scroll(true)
        .rows(rows.build())
        .selected_line(selected_field);
    list.attr(Attribute::Focus, AttrValue::Flag(true));
    list.view(frame, area);

    let content_x = area.x.saturating_add(1);
    let content_y = area.y.saturating_add(1);
    let content_width = area.width.saturating_sub(2);
    let max_rows = area.height.saturating_sub(2) as usize;
    for index in 0..app.repos.len().min(max_rows) {
        app.interaction_map.register_click(
            InteractionLayer::Base,
            Rect::new(content_x, content_y + index as u16, content_width, 1),
            Message::SettingsSelectRepo(index),
        );
    }
}

fn render_settings_footer(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let theme = app.theme;

    if let Some(notice) = &app.footer_notice {
        let mut footer = Label::default()
            .text(notice.as_str())
            .alignment(Alignment::Center)
            .foreground(theme.base.header)
            .background(theme.base.surface);
        footer.view(frame, area);
        return;
    }

    let active_section = app
        .settings_view_state
        .as_ref()
        .map(|s| s.active_section)
        .unwrap_or(SettingsSection::General);

    let help_text = match active_section {
        SettingsSection::General => {
            "j/k: select  h/←: adjust  l/→: adjust  0: reset  Tab: section  Esc: close"
        }
        SettingsSection::CategoryColors => {
            "j/k: select category  Space/Enter: cycle color  Tab: section  Esc: close"
        }
        SettingsSection::Keybindings => "j/k: scroll  Tab: section  Esc: close",
        SettingsSection::Repos => "j/k: select  r: rename  x: remove  Tab: section  Esc: close",
    };

    let mut footer = Label::default()
        .text(help_text)
        .alignment(Alignment::Center)
        .foreground(theme.base.text_muted)
        .background(theme.base.surface);
    footer.view(frame, area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::projects::ProjectInfo;
    use crate::types::SessionTodoItem;
    use uuid::Uuid;

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

    #[test]
    fn test_header_title_includes_project_name() {
        assert_eq!(
            header_title("client-api", false),
            "opencode-kanban [client-api]"
        );
        assert_eq!(
            header_title("client-api", true),
            "opencode-kanban [client-api] [CATEGORY EDIT]"
        );
    }

    #[test]
    fn test_resolve_header_project_name_prefers_project_list_name() {
        let path = std::path::PathBuf::from("/tmp/dbs/opencode-kanban.sqlite");
        let projects = vec![ProjectInfo {
            name: "workspace-main".to_string(),
            path: path.clone(),
        }];

        assert_eq!(
            resolve_header_project_name(Some(path.as_path()), &projects),
            "workspace-main"
        );
    }

    #[test]
    fn test_resolve_header_project_name_falls_back_to_path_stem() {
        let path = std::path::PathBuf::from("/tmp/dbs/solo.sqlite");
        let projects = Vec::new();

        assert_eq!(
            resolve_header_project_name(Some(path.as_path()), &projects),
            "solo"
        );
    }

    #[test]
    fn test_side_panel_selected_line_accounts_for_header_and_tile_rows() {
        let category_id = Uuid::new_v4();
        let rows = vec![
            SidePanelRow::CategoryHeader {
                column_index: 0,
                category_id,
                category_name: "TODO".to_string(),
                category_color: None,
                total_tasks: 2,
                visible_tasks: 2,
                collapsed: false,
            },
            SidePanelRow::Task {
                column_index: 0,
                index_in_column: 0,
                category_id,
                task: Box::new(test_task(category_id, 0)),
            },
            SidePanelRow::Task {
                column_index: 0,
                index_in_column: 1,
                category_id,
                task: Box::new(test_task(category_id, 1)),
            },
        ];

        assert_eq!(side_panel_selected_line(&rows, 0), 0);
        assert_eq!(side_panel_selected_line(&rows, 1), 5);
        assert_eq!(side_panel_selected_line(&rows, 2), 10);
    }

    #[test]
    fn test_column_selected_line_tracks_bottom_of_selected_tile() {
        let category_id = Uuid::new_v4();
        let tasks = vec![
            test_task(category_id, 0),
            test_task(category_id, 1),
            test_task(category_id, 2),
        ];

        assert_eq!(column_selected_line(&tasks, 0), 4);
        assert_eq!(column_selected_line(&tasks, 1), 9);
        assert_eq!(column_selected_line(&tasks, 2), 14);
        assert_eq!(column_selected_line(&tasks, 99), 14);
    }

    #[test]
    fn test_column_selected_line_returns_zero_when_no_tasks() {
        assert_eq!(column_selected_line(&[], 0), 0);
    }

    #[test]
    fn test_column_scroll_offset_when_selection_fits_viewport() {
        assert_eq!(column_scroll_offset(2, 15, 10), 0);
    }

    #[test]
    fn test_column_scroll_offset_clamps_to_max_offset() {
        assert_eq!(column_scroll_offset(14, 15, 5), 10);
    }

    #[test]
    fn test_scrollbar_position_for_offset_maps_to_full_range() {
        assert_eq!(scrollbar_position_for_offset(0, 60, 48), 0);
        assert_eq!(scrollbar_position_for_offset(12, 60, 48), 59);
    }

    #[test]
    fn test_todo_checklist_lines_use_expected_markers() {
        let todos = vec![
            SessionTodoItem {
                content: "done".to_string(),
                completed: true,
            },
            SessionTodoItem {
                content: "active".to_string(),
                completed: false,
            },
            SessionTodoItem {
                content: "pending".to_string(),
                completed: false,
            },
        ];

        let lines = todo_checklist_lines(&todos);
        assert_eq!(lines.len(), 3);
        assert!(lines[0].0.contains("[✓] done"));
        assert_eq!(lines[0].1, TodoLineState::Completed);
        assert!(lines[1].0.contains("[•] active"));
        assert_eq!(lines[1].1, TodoLineState::Active);
        assert!(lines[2].0.contains("[ ] pending"));
        assert_eq!(lines[2].1, TodoLineState::Pending);
    }

    #[test]
    fn test_todo_checklist_lines_show_pending_when_all_incomplete() {
        let todos = vec![
            SessionTodoItem {
                content: "first".to_string(),
                completed: false,
            },
            SessionTodoItem {
                content: "second".to_string(),
                completed: false,
            },
        ];

        let lines = todo_checklist_lines(&todos);
        assert!(lines[0].0.contains("[•] first"));
        assert!(lines[1].0.contains("[ ] second"));
    }

    #[test]
    fn test_delete_task_checkbox_focus_index_maps_fields() {
        assert_eq!(
            delete_task_checkbox_focus_index(DeleteTaskField::KillTmux),
            Some(0)
        );
        assert_eq!(
            delete_task_checkbox_focus_index(DeleteTaskField::RemoveWorktree),
            Some(1)
        );
        assert_eq!(
            delete_task_checkbox_focus_index(DeleteTaskField::DeleteBranch),
            Some(2)
        );
        assert_eq!(
            delete_task_checkbox_focus_index(DeleteTaskField::Delete),
            None
        );
        assert_eq!(
            delete_task_checkbox_focus_index(DeleteTaskField::Cancel),
            None
        );
    }

    #[test]
    fn test_set_checkbox_highlight_choice_uses_stdlib_navigation() {
        let mut checkbox = Checkbox::default()
            .choices(["Kill tmux", "Remove worktree", "Delete branch"])
            .values(&[])
            .rewind(false);

        set_checkbox_highlight_choice(&mut checkbox, Some(2));
        assert_eq!(checkbox.states.choice, 2);

        set_checkbox_highlight_choice(&mut checkbox, None);
        assert_eq!(checkbox.states.choice, 2);
    }

    #[test]
    fn test_text_input_cursor_position_clamps_to_visible_input_width() {
        let area = Rect::new(10, 6, 8, 3);
        let (x, y) = text_input_cursor_position(area, "feature/long-branch")
            .expect("cursor should be set for valid input area");
        assert_eq!(x, 16);
        assert_eq!(y, 7);
    }

    #[test]
    fn test_text_input_cursor_position_none_for_tiny_area() {
        assert_eq!(
            text_input_cursor_position(Rect::new(0, 0, 2, 2), "abc"),
            None
        );
    }

    #[test]
    fn test_resolve_input_display_value_uses_placeholder_when_empty() {
        let (display, using_placeholder) =
            resolve_input_display_value("", Some("auto-generated if empty"));
        assert_eq!(display, "auto-generated if empty");
        assert!(using_placeholder);
    }

    #[test]
    fn test_resolve_input_display_value_prefers_user_value() {
        let (display, using_placeholder) =
            resolve_input_display_value("feature/manual", Some("auto-generated if empty"));
        assert_eq!(display, "feature/manual");
        assert!(!using_placeholder);
    }

    #[test]
    fn test_effective_scroll_column_width_respects_viewport_guard() {
        assert_eq!(effective_scroll_column_width(42, 120), 42);
        assert_eq!(effective_scroll_column_width(42, 20), 16);
    }

    #[test]
    fn test_focused_viewport_offset_moves_right_with_peek_when_neighbor_exists() {
        let offset = focused_viewport_offset(0, 80, 40, 1, 6, 3);
        assert_eq!(offset, 89);
    }

    #[test]
    fn test_focused_viewport_offset_keeps_last_column_stable_without_right_peek() {
        let offset = focused_viewport_offset(84, 80, 40, 1, 4, 3);
        assert_eq!(offset, 83);
    }

    #[test]
    fn test_focused_viewport_offset_uses_left_peek_when_not_on_first_column() {
        let offset = focused_viewport_offset(40, 80, 40, 1, 4, 1);
        assert_eq!(offset, 35);
    }

    fn test_task(category_id: Uuid, position: i64) -> Task {
        Task {
            id: Uuid::new_v4(),
            title: "Task".to_string(),
            repo_id: Uuid::new_v4(),
            branch: "feature/test".to_string(),
            category_id,
            position,
            tmux_session_name: None,
            worktree_path: None,
            tmux_status: "idle".to_string(),
            status_source: "none".to_string(),
            status_fetched_at: None,
            status_error: None,
            opencode_session_id: None,
            attach_overlay_shown: false,
            archived: false,
            archived_at: None,
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
        }
    }
}
