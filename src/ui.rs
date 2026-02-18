use tui_realm_stdlib::{Checkbox, Input, Label, List, Paragraph, Table};
use tuirealm::{
    MockComponent,
    props::{
        Alignment, AttrValue, Attribute, BorderType, Borders, Color, InputType, Style,
        TableBuilder, TextSpan,
    },
    ratatui::{
        Frame,
        layout::{Constraint, Direction, Layout, Rect},
        style::Style as RatatuiStyle,
        widgets::{Clear, Scrollbar, ScrollbarOrientation, ScrollbarState},
    },
};

use crate::app::{
    ActiveDialog, App, ArchiveTaskDialogState, CATEGORY_COLOR_PALETTE, CategoryColorField,
    CategoryInputField, CategoryInputMode, ConfirmCancelField, DeleteProjectDialogState,
    DeleteRepoDialogState, DeleteTaskField, NewProjectDialogState, NewProjectField, NewTaskField,
    ProjectDetailCache, RenameProjectDialogState, RenameProjectField, RenameRepoDialogState,
    RenameRepoField, SettingsSection, SidePanelRow, TodoVisualizationMode, View, ViewMode,
    category_color_label,
};
use crate::command_palette::all_commands;
use crate::theme::Theme;
use crate::types::{Category, SessionTodoItem, Task};

#[derive(Clone, Copy)]
pub enum OverlayAnchor {
    Center,
    Top,
}

pub fn render(frame: &mut Frame<'_>, app: &mut App) {
    app.hit_test_map.clear();

    match app.current_view {
        View::ProjectList => render_project_list(frame, app),
        View::Board => render_board(frame, app),
        View::Settings => render_settings(frame, app),
        View::Archive => render_archive(frame, app),
    }

    if app.active_dialog != ActiveDialog::None {
        render_dialog(frame, app);
    }
}

fn render_project_list(frame: &mut Frame<'_>, app: &App) {
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
        let is_active = app
            .current_project_path
            .as_ref()
            .map(|p| p == &project.path)
            .unwrap_or(false);
        let marker = if is_active { "*" } else { " " };
        let name = format!("{} {}", marker, project.name);
        if is_selected {
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

    render_project_detail_panel(frame, content[1], app);

    let mut footer = Label::default()
        .text("n: new  r: rename  x: delete  Enter: open  j/k: navigate  q: quit")
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
            TextSpan::new("  Enter open  r rename  x delete  n new").fg(theme.base.text_muted),
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

fn render_board(frame: &mut Frame<'_>, app: &App) {
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
    render_footer(frame, chunks[2], app);
}

fn render_archive(frame: &mut Frame<'_>, app: &App) {
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

    let mut header = Label::default()
        .text(format!("Archive ({})", app.archived_tasks.len()))
        .alignment(Alignment::Left)
        .foreground(theme.base.header)
        .background(theme.base.surface);
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
            .map(|value| clamp_text(value, 19))
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
        vec![
            TextSpan::new("ARCHIVED TASK").fg(theme.base.header).bold(),
            TextSpan::new(detail_kv("Title", task.title.as_str())).fg(theme.base.text),
            TextSpan::new(detail_kv("Repo", repo_name)).fg(theme.base.text),
            TextSpan::new(detail_kv("Branch", task.branch.as_str())).fg(theme.base.text),
            TextSpan::new(detail_kv("Category", category_name)).fg(theme.base.text),
            TextSpan::new(detail_kv(
                "Archived",
                task.archived_at.as_deref().unwrap_or("unknown"),
            ))
            .fg(theme.base.text_muted),
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
        .background(theme.base.surface)
        .wrap(true)
        .text(details_lines);
    details.view(frame, body[1]);

    let mut footer = Label::default()
        .text("j/k:select  u:unarchive  d:delete  Esc:back")
        .alignment(Alignment::Center)
        .foreground(theme.base.text_muted)
        .background(theme.base.surface);
    footer.view(frame, chunks[2]);
}

fn render_header(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let theme = app.theme;
    let sections = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(65), Constraint::Percentage(35)])
        .split(area);

    let title = if app.category_edit_mode {
        "opencode-kanban [CATEGORY EDIT]"
    } else {
        "opencode-kanban"
    };

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

fn render_footer(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let theme = app.theme;
    let notice = if let Some(notice) = &app.footer_notice {
        notice.as_str()
    } else if app.category_edit_mode {
        "EDIT MODE  h/l:nav  H/L:reorder  p:color  r:rename  x:delete  g:exit"
    } else {
        match app.view_mode {
            ViewMode::Kanban => {
                "n:new  a:archive  A:archive view  Enter:attach  t:todo view  Ctrl+P:palette  c/r/x/p:category  H/L move  J/K reorder  v:view"
            }
            ViewMode::SidePanel => {
                "j/k:select  Space:collapse  a:archive  A:archive view  Enter:attach task  t:todo view  c/r/x/p:category  H/L/J/K:move  v:view"
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

fn render_columns(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let theme = app.theme;
    if app.categories.is_empty() {
        render_empty_state(frame, area, "No categories yet. Press c to add one.", app);
        return;
    }

    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(vec![
            Constraint::Ratio(1, app.categories.len() as u32);
            app.categories.len()
        ])
        .split(area);

    for (slot, (column_idx, category)) in sorted_categories(app).into_iter().enumerate() {
        let mut rows = TableBuilder::default();
        let tasks = tasks_for_category(app, category.id);
        let accent = theme.category_accent(category.color.as_deref());
        let row_count = if tasks.is_empty() {
            1
        } else {
            tasks.iter().map(task_tile_lines).sum()
        };
        let viewport_lines = list_inner_height(columns[slot]);
        let show_scrollbar = viewport_lines > 0 && row_count > viewport_lines;

        let selected_task = app
            .selected_task_per_column
            .get(&column_idx)
            .copied()
            .unwrap_or(0)
            .min(tasks.len().saturating_sub(1));
        let is_focused_column = column_idx == app.focused_column;

        let tile_width =
            list_inner_width(columns[slot]).saturating_sub(usize::from(show_scrollbar));
        for (task_index, task) in tasks.iter().enumerate() {
            append_task_tile_rows(
                &mut rows,
                app,
                task,
                is_focused_column && task_index == selected_task,
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
        list.view(frame, columns[slot]);

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
            let scrollbar_area = inset_rect(columns[slot], 1, 1);
            if scrollbar_area.width > 0 && scrollbar_area.height > 0 {
                frame.render_stateful_widget(scrollbar, scrollbar_area, &mut state);
            }
        }
    }
}

fn render_side_panel(frame: &mut Frame<'_>, area: Rect, app: &App) {
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
    app: &App,
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
                let style = if row_index == selected_row {
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
                    row_index == selected_row,
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
    list.attr(Attribute::Focus, AttrValue::Flag(true));
    list.view(frame, area);

    if show_scrollbar {
        let scroll_offset = column_scroll_offset(selected_line, row_count, viewport_lines);
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
    app: &App,
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

fn render_side_panel_task_details(frame: &mut Frame<'_>, area: Rect, app: &App, task: &Task) {
    let theme = app.theme;

    let repo_name = app
        .repos
        .iter()
        .find(|repo| repo.id == task.repo_id)
        .map(|repo| repo.name.clone())
        .unwrap_or_else(|| "unknown".to_string());

    let spinner = status_spinner_ascii(task.tmux_status.as_str(), app.pulse_phase);
    let todo_summary = app
        .session_todo_summary(task.id)
        .map(|(done, total)| format!("{done}/{total}"))
        .unwrap_or_else(|| "--".to_string());
    let todo_view = app.todo_visualization_mode.as_str();
    let session = task.tmux_session_name.as_deref().unwrap_or("n/a");

    let worktree_full = task.worktree_path.as_deref().unwrap_or("n/a");
    let worktree_short = clamp_text(worktree_full, 70);

    let mut lines = vec![
        TextSpan::new("OVERVIEW").fg(theme.base.header).bold(),
        TextSpan::new(detail_kv("Title", &task.title)).fg(theme.base.text),
        TextSpan::new(detail_kv("Repo", &repo_name)).fg(theme.base.text),
        TextSpan::new(detail_kv("Branch", &task.branch)).fg(theme.base.text),
        TextSpan::new(""),
        TextSpan::new("RUNTIME").fg(theme.base.header).bold(),
        TextSpan::new(detail_kv("Status", spinner))
            .fg(theme.status_color(task.tmux_status.as_str())),
        TextSpan::new(detail_kv("Todos", &todo_summary)).fg(theme.tile.todo),
        TextSpan::new(detail_kv("TodoView", todo_view)).fg(theme.base.text_muted),
        TextSpan::new(detail_kv("Session", session)).fg(theme.base.text),
        TextSpan::new(""),
        TextSpan::new("WORKSPACE").fg(theme.base.header).bold(),
        TextSpan::new(detail_kv("Path", &worktree_short)).fg(theme.base.text),
    ];

    if app.todo_visualization_mode == TodoVisualizationMode::Checklist {
        let task_todos = app.session_todos(task.id);
        let checklist_lines = todo_checklist_lines(&task_todos);
        if !checklist_lines.is_empty() {
            lines.push(TextSpan::new(""));
            lines.push(TextSpan::new("WORK PLAN").fg(theme.base.header).bold());
            for (line, state) in checklist_lines {
                lines.push(TextSpan::new(line).fg(todo_state_color(theme, state)));
            }
        }
    }

    if worktree_full != worktree_short {
        lines.push(TextSpan::new(detail_kv("Full", worktree_full)).fg(theme.base.text_muted));
    }

    lines.push(TextSpan::new(""));
    lines.push(TextSpan::new("ACTIONS").fg(theme.base.header).bold());
    lines.push(TextSpan::new("Enter attach  d delete  m move  l logs").fg(theme.base.text_muted));

    if let Some(log) = app.current_log_buffer.as_deref() {
        lines.push(TextSpan::new(""));
        lines.push(TextSpan::new("LOG PREVIEW").fg(theme.base.header).bold());
        for line in log.lines().take(8) {
            lines.push(TextSpan::new(line.to_string()).fg(theme.base.text_muted));
        }
    }

    let mut paragraph = Paragraph::default()
        .title("Details", Alignment::Left)
        .borders(rounded_borders(theme.interactive.focus))
        .foreground(theme.base.text)
        .background(theme.base.surface)
        .wrap(true)
        .text(lines);
    paragraph.view(frame, area);
}

fn render_side_panel_category_details(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
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
    lines.push(TextSpan::new("Space toggle  j/k navigate  Enter attach on task").fg(accent));

    let mut paragraph = Paragraph::default()
        .title("Category Summary", Alignment::Left)
        .borders(rounded_borders(accent))
        .foreground(theme.base.text)
        .background(theme.base.surface)
        .wrap(true)
        .text(lines);
    paragraph.view(frame, area);
}

fn render_dialog(frame: &mut Frame<'_>, app: &App) {
    if matches!(app.active_dialog, ActiveDialog::Help) {
        render_help_overlay(frame, app);
        return;
    }

    let (width_percent, height_percent) = match &app.active_dialog {
        ActiveDialog::CommandPalette(_) => command_palette_overlay_size(app.viewport),
        ActiveDialog::NewTask(_) => (80, 72),
        ActiveDialog::ArchiveTask(_) => (55, 35),
        ActiveDialog::DeleteTask(_) => (60, 60),
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

    match &app.active_dialog {
        ActiveDialog::NewTask(state) => render_new_task_dialog(frame, dialog_area, app, state),
        ActiveDialog::DeleteTask(state) => {
            render_delete_task_dialog(frame, dialog_area, app, state)
        }
        ActiveDialog::ArchiveTask(state) => {
            render_archive_task_dialog(frame, dialog_area, app, state)
        }
        ActiveDialog::CategoryInput(state) => {
            render_category_dialog(frame, dialog_area, app, state)
        }
        ActiveDialog::CategoryColor(state) => {
            render_category_color_dialog(frame, dialog_area, app, state)
        }
        ActiveDialog::DeleteCategory(state) => {
            render_delete_category_dialog(frame, dialog_area, app, state)
        }
        ActiveDialog::Error(state) => {
            let text = format!("{}\n\n{}", state.title, state.detail);
            render_message_dialog(frame, dialog_area, app, "Error", &text);
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
            render_confirm_quit_dialog(frame, dialog_area, app, state);
        }
        ActiveDialog::CommandPalette(state) => {
            render_command_palette_dialog(frame, dialog_area, app, state)
        }
        ActiveDialog::NewProject(state) => {
            render_new_project_dialog(frame, dialog_area, app, state)
        }
        ActiveDialog::RenameProject(state) => {
            render_rename_project_dialog(frame, dialog_area, app, state)
        }
        ActiveDialog::DeleteProject(state) => {
            render_delete_project_dialog(frame, dialog_area, app, state)
        }
        ActiveDialog::RenameRepo(state) => {
            render_rename_repo_dialog(frame, dialog_area, app, state)
        }
        ActiveDialog::DeleteRepo(state) => {
            render_delete_repo_dialog(frame, dialog_area, app, state)
        }
        ActiveDialog::MoveTask(_) | ActiveDialog::None | ActiveDialog::Help => {}
    }
}

fn render_new_task_dialog(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    state: &crate::app::NewTaskDialogState,
) {
    let theme = app.theme;
    let surface = dialog_surface(theme);

    let mut panel =
        dialog_panel("New Task", Alignment::Center, theme, surface).text([TextSpan::from("")]);
    panel.view(frame, area);

    let panel_inner = inset_rect(area, 1, 1);
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(2),
            Constraint::Min(0),
        ])
        .split(panel_inner);

    render_input_component(
        frame,
        layout[0],
        "Repo",
        if state.repo_input.is_empty() {
            app.repos
                .get(state.repo_idx)
                .map(|repo| repo.name.as_str())
                .unwrap_or("")
        } else {
            state.repo_input.as_str()
        },
        state.focused_field == NewTaskField::Repo,
        surface,
        theme,
    );
    render_input_component(
        frame,
        layout[1],
        "Branch",
        &state.branch_input,
        state.focused_field == NewTaskField::Branch,
        surface,
        theme,
    );
    render_input_component(
        frame,
        layout[2],
        "Base",
        &state.base_input,
        state.focused_field == NewTaskField::Base,
        surface,
        theme,
    );
    render_input_component(
        frame,
        layout[3],
        "Title",
        &state.title_input,
        state.focused_field == NewTaskField::Title,
        surface,
        theme,
    );

    let selected = if state.ensure_base_up_to_date {
        vec![0]
    } else {
        Vec::new()
    };
    let mut checkbox = dialog_checkbox("Options", theme, surface)
        .choices(["Ensure base is up to date"])
        .values(&selected)
        .rewind(false);
    checkbox.attr(
        Attribute::Focus,
        AttrValue::Flag(state.focused_field == NewTaskField::EnsureBaseUpToDate),
    );
    checkbox.view(frame, layout[4]);

    let actions = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(layout[5]);

    render_action_button(
        frame,
        actions[0],
        "Create",
        matches!(state.focused_field, NewTaskField::Create),
        false,
        app,
    );
    render_action_button(
        frame,
        actions[1],
        "Cancel",
        matches!(state.focused_field, NewTaskField::Cancel),
        false,
        app,
    );

    let mut hint = Label::default()
        .text("Tab/Up/Down: move focus  Enter: confirm  Esc: cancel")
        .alignment(Alignment::Center)
        .foreground(theme.base.text_muted)
        .background(surface);
    hint.view(frame, layout[6]);
}

fn render_delete_task_dialog(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
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
            TextSpan::from("Use Space to toggle options."),
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

    let mut checkbox = dialog_checkbox("Delete Options", theme, dialog_surface(theme))
        .choices(["Kill tmux", "Remove worktree", "Delete branch"])
        .values(&selected)
        .rewind(false);
    checkbox.attr(
        Attribute::Focus,
        AttrValue::Flag(matches!(
            state.focused_field,
            DeleteTaskField::KillTmux
                | DeleteTaskField::RemoveWorktree
                | DeleteTaskField::DeleteBranch
        )),
    );
    checkbox.view(frame, layout[1]);

    let buttons = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(layout[3]);

    render_action_button(
        frame,
        buttons[0],
        "Delete",
        matches!(state.focused_field, DeleteTaskField::Delete),
        true,
        app,
    );
    render_action_button(
        frame,
        buttons[1],
        "Cancel",
        matches!(state.focused_field, DeleteTaskField::Cancel),
        false,
        app,
    );
}

fn render_archive_task_dialog(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
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
        },
    );
}

fn render_category_dialog(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
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
        surface,
        theme,
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
    );
    render_action_button(
        frame,
        buttons[1],
        "Cancel",
        matches!(state.focused_field, CategoryInputField::Cancel),
        false,
        app,
    );

    let mut hint = Label::default()
        .text("Tab: next field  Enter: confirm  Esc: cancel")
        .alignment(Alignment::Center)
        .foreground(theme.base.text_muted)
        .background(surface);
    hint.view(frame, layout[3]);
}

fn render_delete_category_dialog(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
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
        },
    );
}

fn render_confirm_quit_dialog(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
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
        },
    );
}

struct ConfirmCancelDialogSpec<'a> {
    title: &'a str,
    text: &'a str,
    confirm_label: &'a str,
    confirm_destructive: bool,
    focused_field: ConfirmCancelField,
}

fn render_confirm_cancel_dialog(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
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
    );
    render_action_button(
        frame,
        buttons[1],
        "Cancel",
        matches!(spec.focused_field, ConfirmCancelField::Cancel),
        false,
        app,
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
    app: &App,
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
        surface,
        theme,
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
    );
    render_action_button(
        frame,
        buttons[1],
        "Cancel",
        matches!(state.focused_field, NewProjectField::Cancel),
        false,
        app,
    );

    let mut hint = Label::default()
        .text("Tab: next field  Enter: confirm  Esc: cancel")
        .alignment(Alignment::Center)
        .foreground(theme.base.text_muted)
        .background(surface);
    hint.view(frame, layout[3]);
}

fn render_rename_project_dialog(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
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
        surface,
        theme,
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
    );
    render_action_button(
        frame,
        buttons[1],
        "Cancel",
        matches!(state.focused_field, RenameProjectField::Cancel),
        false,
        app,
    );

    let mut hint = Label::default()
        .text("Tab: next field  Enter: confirm  Esc: cancel")
        .alignment(Alignment::Center)
        .foreground(theme.base.text_muted)
        .background(surface);
    hint.view(frame, layout[3]);
}

fn render_delete_project_dialog(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
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

    render_action_button(frame, buttons[0], "Delete", true, true, app);
    render_action_button(frame, buttons[1], "Cancel", false, false, app);

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
    app: &App,
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
        surface,
        theme,
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
    );
    render_action_button(
        frame,
        buttons[1],
        "Cancel",
        matches!(state.focused_field, RenameRepoField::Cancel),
        false,
        app,
    );

    let mut hint = Label::default()
        .text("Tab: next field  Enter: confirm  Esc: cancel")
        .alignment(Alignment::Center)
        .foreground(theme.base.text_muted)
        .background(surface);
    hint.view(frame, layout[3]);
}

fn render_delete_repo_dialog(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
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

    render_action_button(frame, buttons[0], "Remove", true, true, app);
    render_action_button(frame, buttons[1], "Cancel", false, false, app);

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
    app: &App,
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
    );
    render_action_button(
        frame,
        buttons[1],
        "Cancel",
        matches!(state.focused_field, CategoryColorField::Cancel),
        false,
        app,
    );

    let mut hint = Label::default()
        .text("Arrows/jk: navigate  Tab: next field  Enter: confirm  Esc: cancel")
        .alignment(Alignment::Center)
        .foreground(theme.base.text_muted)
        .background(surface);
    hint.view(frame, layout[3]);
}

fn render_message_dialog(frame: &mut Frame<'_>, area: Rect, app: &App, title: &str, text: &str) {
    let theme = app.theme;
    let mut paragraph = dialog_panel(title, Alignment::Center, theme, dialog_surface(theme))
        .wrap(true)
        .text(text.lines().map(|line| TextSpan::from(line.to_string())));
    paragraph.view(frame, area);
}

fn render_action_button(
    frame: &mut Frame<'_>,
    area: Rect,
    label: &str,
    focused: bool,
    destructive: bool,
    app: &App,
) {
    let theme = app.theme;
    let (accent, fg, bg) = dialog_button_palette(theme, focused, destructive);

    let mut button = Paragraph::default()
        .borders(rounded_borders(accent))
        .foreground(fg)
        .background(if focused { bg } else { dialog_surface(theme) })
        .alignment(Alignment::Center)
        .text([TextSpan::from(label.to_string())]);
    button.view(frame, area);
}

fn render_command_palette_dialog(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
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
        dialog_surface(theme),
        theme,
    );

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
    background: Color,
    theme: Theme,
) {
    let mut input = Input::default()
        .title(title, Alignment::Left)
        .borders(rounded_borders(dialog_input_border(theme, focused)))
        .foreground(theme.base.text)
        .background(background)
        .inactive(Style::default().fg(theme.base.text_muted))
        .input_type(InputType::Text)
        .value(value.to_string());
    input.attr(Attribute::Focus, AttrValue::Flag(focused));
    input.view(frame, area);
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

fn sorted_categories(app: &App) -> Vec<(usize, &Category)> {
    let mut categories: Vec<(usize, &Category)> = app.categories.iter().enumerate().collect();
    categories.sort_by_key(|(_, category)| category.position);
    categories
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

fn render_settings(frame: &mut Frame<'_>, app: &App) {
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

fn render_settings_content(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let sections = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(20), Constraint::Percentage(80)])
        .split(area);

    render_settings_sidebar(frame, sections[0], app);
    render_settings_active_section(frame, sections[1], app);
}

fn render_settings_sidebar(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let theme = app.theme;
    let active_section = app
        .settings_view_state
        .as_ref()
        .map(|s| s.active_section)
        .unwrap_or(SettingsSection::Theme);

    let mut rows = TableBuilder::default();
    for section in [
        SettingsSection::Theme,
        SettingsSection::CategoryColors,
        SettingsSection::Keybindings,
        SettingsSection::General,
        SettingsSection::Repos,
    ] {
        let label = match section {
            SettingsSection::Theme => "Theme",
            SettingsSection::CategoryColors => "Category Colors",
            SettingsSection::Keybindings => "Keybindings",
            SettingsSection::General => "General",
            SettingsSection::Repos => "Repos",
        };
        let prefix = if section == active_section {
            "> "
        } else {
            "  "
        };
        rows.add_col(TextSpan::from(format!("{}{}", prefix, label)))
            .add_row();
    }

    let selected_idx = match active_section {
        SettingsSection::Theme => 0,
        SettingsSection::CategoryColors => 1,
        SettingsSection::Keybindings => 2,
        SettingsSection::General => 3,
        SettingsSection::Repos => 4,
    };

    let mut list = List::default()
        .title("Settings", Alignment::Left)
        .borders(rounded_borders(theme.interactive.focus))
        .foreground(theme.base.text)
        .highlighted_color(theme.interactive.focus)
        .highlighted_str("> ")
        .scroll(false)
        .rows(rows.build())
        .selected_line(selected_idx);
    list.attr(Attribute::Focus, AttrValue::Flag(true));
    list.view(frame, area);
}

fn render_settings_active_section(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let active_section = app
        .settings_view_state
        .as_ref()
        .map(|s| s.active_section)
        .unwrap_or(SettingsSection::Theme);
    match active_section {
        SettingsSection::Theme => render_settings_theme(frame, area, app),
        SettingsSection::CategoryColors => render_settings_category_colors(frame, area, app),
        SettingsSection::Keybindings => render_settings_keybindings(frame, area, app),
        SettingsSection::General => render_settings_general(frame, area, app),
        SettingsSection::Repos => render_settings_repos(frame, area, app),
    }
}

fn render_settings_category_colors(frame: &mut Frame<'_>, area: Rect, app: &App) {
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
            let prefix = if index == selected_field { "> " } else { "  " };
            let color_label = category_color_label(category.color.as_deref());
            rows.add_col(
                TextSpan::new(format!("{}{}: {}", prefix, category.name, color_label))
                    .fg(theme.category_accent(category.color.as_deref())),
            )
            .add_row();
        }
    }

    let mut list = List::default()
        .title("Category Colors", Alignment::Left)
        .borders(rounded_borders(theme.interactive.focus))
        .foreground(theme.base.text)
        .highlighted_color(theme.interactive.focus)
        .highlighted_str("> ")
        .scroll(false)
        .rows(rows.build())
        .selected_line(selected_field);
    list.attr(Attribute::Focus, AttrValue::Flag(true));
    list.view(frame, area);
}

fn render_settings_theme(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let theme = app.theme;
    let current_theme = &app.settings.theme;

    let mut rows = TableBuilder::default();
    for preset in ["default", "high-contrast", "mono"] {
        let is_selected = current_theme == preset;
        let prefix = if is_selected { " [x] " } else { " [ ] " };
        rows.add_col(TextSpan::from(format!("{}{}", prefix, preset)))
            .add_row();
    }

    let selected_idx = match current_theme.as_str() {
        "default" => 0,
        "high-contrast" => 1,
        "mono" => 2,
        _ => 0,
    };

    let mut list = List::default()
        .title("Theme", Alignment::Left)
        .borders(rounded_borders(theme.interactive.focus))
        .foreground(theme.base.text)
        .highlighted_color(theme.interactive.focus)
        .highlighted_str("> ")
        .scroll(false)
        .rows(rows.build())
        .selected_line(selected_idx);
    list.attr(Attribute::Focus, AttrValue::Flag(true));
    list.view(frame, area);
}

fn render_settings_keybindings(frame: &mut Frame<'_>, area: Rect, app: &App) {
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
        .scroll(true)
        .rows(rows.build());
    list.attr(Attribute::Focus, AttrValue::Flag(true));
    list.view(frame, area);
}

fn render_settings_general(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let theme = app.theme;
    let selected_field = app
        .settings_view_state
        .as_ref()
        .map(|s| s.general_selected_field)
        .unwrap_or(0);

    let mut rows = TableBuilder::default();

    let poll_prefix = if selected_field == 0 { "> " } else { "  " };
    rows.add_col(TextSpan::from(format!(
        "{}Poll Interval: {} ms",
        poll_prefix, app.settings.poll_interval_ms
    )))
    .add_row();

    let width_prefix = if selected_field == 1 { "> " } else { "  " };
    rows.add_col(TextSpan::from(format!(
        "{}Side Panel Width: {}%",
        width_prefix, app.settings.side_panel_width
    )))
    .add_row();

    let mut list = List::default()
        .title("General", Alignment::Left)
        .borders(rounded_borders(theme.interactive.focus))
        .foreground(theme.base.text)
        .highlighted_color(theme.interactive.focus)
        .highlighted_str("> ")
        .scroll(false)
        .rows(rows.build())
        .selected_line(selected_field);
    list.attr(Attribute::Focus, AttrValue::Flag(true));
    list.view(frame, area);
}

fn render_settings_repos(frame: &mut Frame<'_>, area: Rect, app: &App) {
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
            let prefix = if index == selected_field { "> " } else { "  " };
            let path_short = clamp_text(&repo.path, 45);
            let base = repo
                .default_base
                .as_deref()
                .filter(|b| !b.is_empty())
                .unwrap_or("—");
            rows.add_col(TextSpan::new(format!(
                "{}{:<20} {} [{}]",
                prefix,
                clamp_text(&repo.name, 20),
                path_short,
                base
            )))
            .add_row();
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
}

fn render_settings_footer(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let theme = app.theme;
    let active_section = app
        .settings_view_state
        .as_ref()
        .map(|s| s.active_section)
        .unwrap_or(SettingsSection::Theme);

    let help_text = match active_section {
        SettingsSection::Theme => "Space/Enter: cycle theme  h/l: section  Esc: close",
        SettingsSection::CategoryColors => {
            "j/k: select category  Space/Enter: cycle color  h/l: section  Esc: close"
        }
        SettingsSection::Keybindings => "h/l: section  Esc: close",
        SettingsSection::General => "j/k: select  h/l: section  Esc: close",
        SettingsSection::Repos => "j/k: select  r: rename  x: remove  h/l: section  Esc: close",
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
            archived: false,
            archived_at: None,
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
        }
    }
}
