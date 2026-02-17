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
        widgets::Clear,
    },
};

use crate::app::{
    ActiveDialog, App, CategoryInputMode, DeleteTaskField, NewTaskField, View, ViewMode,
};
use crate::command_palette::all_commands;
use crate::theme::{Theme, parse_color};
use crate::types::{Category, Task};

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
    }

    if app.active_dialog != ActiveDialog::None {
        render_dialog(frame, app);
    }
}

fn render_project_list(frame: &mut Frame<'_>, app: &App) {
    let theme = Theme::default();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(2),
        ])
        .split(frame.area());

    let mut header = Label::default()
        .text("Select Project")
        .alignment(Alignment::Center)
        .foreground(theme.header)
        .background(Color::Black);
    header.view(frame, chunks[0]);

    let mut rows = TableBuilder::default();
    for (idx, project) in app.project_list.iter().enumerate() {
        let prefix = if idx == app.selected_project_index {
            ">"
        } else {
            " "
        };
        rows.add_col(TextSpan::from(prefix))
            .add_col(TextSpan::from(project.name.clone()))
            .add_row();
    }
    if app.project_list.is_empty() {
        rows.add_col(TextSpan::from(" "))
            .add_col(TextSpan::from("No projects found"))
            .add_row();
    }

    let selected = app
        .selected_project_index
        .min(app.project_list.len().saturating_sub(1));
    let mut list = List::default()
        .title("Projects", Alignment::Left)
        .borders(rounded_borders(theme.focus))
        .foreground(theme.task)
        .highlighted_color(theme.focus)
        .highlighted_str("> ")
        .scroll(true)
        .rows(rows.build())
        .selected_line(selected);
    list.attr(Attribute::Focus, AttrValue::Flag(true));
    list.view(frame, chunks[1]);

    let mut footer = Label::default()
        .text("n: new project  Enter: select  q: quit")
        .alignment(Alignment::Center)
        .foreground(theme.secondary)
        .background(Color::Black);
    footer.view(frame, chunks[2]);
}

fn render_board(frame: &mut Frame<'_>, app: &App) {
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

fn render_header(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let theme = Theme::default();
    let sections = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(65), Constraint::Percentage(35)])
        .split(area);

    let mut left = Label::default()
        .text("opencode-kanban")
        .alignment(Alignment::Left)
        .foreground(theme.header)
        .background(Color::Black);
    left.view(frame, sections[0]);

    let right_text = format!("tasks: {}  refresh: 0.5s", app.tasks.len());
    let mut right = Label::default()
        .text(right_text)
        .alignment(Alignment::Right)
        .foreground(theme.secondary)
        .background(Color::Black);
    right.view(frame, sections[1]);
}

fn render_footer(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let theme = Theme::default();
    let notice = app.footer_notice.as_deref().unwrap_or(
        "n:new  Enter:attach  Ctrl+P:palette  c/r/x:category  H/L move  J/K reorder  v:view",
    );

    let mut footer = Label::default()
        .text(notice)
        .alignment(Alignment::Center)
        .foreground(theme.secondary)
        .background(Color::Black);
    footer.view(frame, area);
}

fn render_columns(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let theme = Theme::default();
    if app.categories.is_empty() {
        render_empty_state(frame, area, "No categories yet. Press c to add one.");
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

        for task in &tasks {
            let status = task.tmux_status.clone();
            let title = format!("{} [{}]", task.title, status);
            rows.add_col(TextSpan::from(title))
                .add_col(TextSpan::from(task.branch.clone()))
                .add_row();
        }

        if tasks.is_empty() {
            rows.add_col(TextSpan::from("No tasks"))
                .add_col(TextSpan::from(""))
                .add_row();
        }

        let selected = app
            .selected_task_per_column
            .get(&column_idx)
            .copied()
            .unwrap_or(0)
            .min(tasks.len().saturating_sub(1));

        let accent = category
            .color
            .as_deref()
            .and_then(parse_color)
            .unwrap_or(theme.column);

        let mut table = Table::default()
            .title(
                format!("{} ({})", category.name, tasks.len()),
                Alignment::Left,
            )
            .borders(rounded_borders(accent))
            .foreground(accent)
            .highlighted_color(theme.focus)
            .highlighted_str("> ")
            .headers(["Task", "Branch"])
            .widths(&[72, 28])
            .scroll(true)
            .table(rows.build())
            .selected_line(selected)
            .inactive(Style::default().fg(theme.secondary));
        table.attr(
            Attribute::Focus,
            AttrValue::Flag(column_idx == app.focused_column),
        );
        table.view(frame, columns[slot]);
    }
}

fn render_side_panel(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let entries = linear_entries(app);
    if entries.is_empty() {
        render_empty_state(frame, area, "No tasks available.");
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

    render_side_panel_list(frame, sections[0], app, &entries);
    render_side_panel_details(frame, sections[1], app, &entries);
}

fn render_side_panel_list(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    entries: &[(String, Task)],
) {
    let theme = Theme::default();
    let mut rows = TableBuilder::default();
    for (category_name, task) in entries {
        rows.add_col(TextSpan::from(task.title.clone()))
            .add_col(TextSpan::from(category_name.clone()))
            .add_row();
    }

    let selected = app.selected_task_index.min(entries.len().saturating_sub(1));
    let mut table = Table::default()
        .title("Tasks", Alignment::Left)
        .borders(rounded_borders(theme.focus))
        .foreground(theme.task)
        .highlighted_color(theme.focus)
        .highlighted_str("> ")
        .headers(["Task", "Category"])
        .widths(&[65, 35])
        .scroll(true)
        .table(rows.build())
        .selected_line(selected)
        .inactive(Style::default().fg(theme.secondary));
    table.attr(Attribute::Focus, AttrValue::Flag(true));
    table.view(frame, area);
}

fn render_side_panel_details(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    entries: &[(String, Task)],
) {
    let theme = Theme::default();
    let selected = app.selected_task_index.min(entries.len().saturating_sub(1));
    let (_, task) = &entries[selected];

    let repo_name = app
        .repos
        .iter()
        .find(|repo| repo.id == task.repo_id)
        .map(|repo| repo.name.clone())
        .unwrap_or_else(|| "unknown".to_string());

    let mut lines = vec![
        TextSpan::from(format!("Title: {}", task.title)),
        TextSpan::from(format!("Branch: {}", task.branch)),
        TextSpan::from(format!("Repo: {}", repo_name)),
        TextSpan::from(format!("Status: {}", task.tmux_status)),
        TextSpan::from(format!(
            "Worktree: {}",
            task.worktree_path.as_deref().unwrap_or("n/a")
        )),
    ];

    if let Some((done, total)) = task.session_todo_summary() {
        lines.push(TextSpan::from(format!("Todos: {}/{}", done, total)));
    }

    if let Some(log) = app.current_log_buffer.as_deref() {
        lines.push(TextSpan::from(""));
        lines.push(TextSpan::from("Recent tmux output:"));
        for line in log.lines().take(20) {
            lines.push(TextSpan::from(line.to_string()));
        }
    }

    let mut paragraph = Paragraph::default()
        .title("Details", Alignment::Left)
        .borders(rounded_borders(theme.focus))
        .foreground(theme.task)
        .wrap(true)
        .text(lines);
    paragraph.view(frame, area);
}

fn render_dialog(frame: &mut Frame<'_>, app: &App) {
    if matches!(app.active_dialog, ActiveDialog::Help) {
        render_help_overlay(frame);
        return;
    }

    let (width_percent, height_percent) = match &app.active_dialog {
        ActiveDialog::CommandPalette(_) => command_palette_overlay_size(app.viewport),
        ActiveDialog::NewTask(_) => (80, 72),
        ActiveDialog::DeleteTask(_) => (60, 60),
        ActiveDialog::CategoryInput(_) => (60, 40),
        ActiveDialog::DeleteCategory(_) => (60, 40),
        ActiveDialog::NewProject(_) => (60, 35),
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
        ActiveDialog::DeleteTask(state) => render_delete_task_dialog(frame, dialog_area, state),
        ActiveDialog::CategoryInput(state) => {
            render_category_dialog(frame, dialog_area, app, state.mode, &state.name_input)
        }
        ActiveDialog::DeleteCategory(state) => {
            let text = format!(
                "Delete category '{}' and {} tasks?\n\nPress Enter to confirm or Esc to cancel.",
                state.category_name, state.task_count
            );
            render_message_dialog(frame, dialog_area, "Delete Category", &text);
        }
        ActiveDialog::Error(state) => {
            let text = format!("{}\n\n{}", state.title, state.detail);
            render_message_dialog(frame, dialog_area, "Error", &text);
        }
        ActiveDialog::WorktreeNotFound(state) => {
            let text = format!(
                "Worktree missing for task '{}'.\n\nEnter: recreate  m: mark broken  Esc: cancel",
                state.task_title
            );
            render_message_dialog(frame, dialog_area, "Worktree Not Found", &text);
        }
        ActiveDialog::RepoUnavailable(state) => {
            let text = format!(
                "Repository unavailable for '{}'.\nPath: {}\n\nPress Enter or Esc.",
                state.task_title, state.repo_path
            );
            render_message_dialog(frame, dialog_area, "Repository Unavailable", &text);
        }
        ActiveDialog::ConfirmQuit(state) => {
            let text = format!(
                "{} active sessions detected.\n\nPress Enter to quit or Esc to cancel.",
                state.active_session_count
            );
            render_message_dialog(frame, dialog_area, "Confirm Quit", &text);
        }
        ActiveDialog::CommandPalette(state) => {
            render_command_palette_dialog(frame, dialog_area, app, state)
        }
        ActiveDialog::NewProject(state) => render_new_project_dialog(frame, dialog_area, state),
        ActiveDialog::MoveTask(_) | ActiveDialog::None | ActiveDialog::Help => {}
    }
}

fn render_new_task_dialog(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    state: &crate::app::NewTaskDialogState,
) {
    let theme = Theme::default();
    let surface = overlay_surface_color();

    let mut panel = Paragraph::default()
        .title("New Task", Alignment::Center)
        .borders(rounded_borders(theme.focus))
        .foreground(theme.task)
        .background(surface)
        .text([TextSpan::from("")]);
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
    );
    render_input_component(
        frame,
        layout[1],
        "Branch",
        &state.branch_input,
        state.focused_field == NewTaskField::Branch,
        surface,
    );
    render_input_component(
        frame,
        layout[2],
        "Base",
        &state.base_input,
        state.focused_field == NewTaskField::Base,
        surface,
    );
    render_input_component(
        frame,
        layout[3],
        "Title",
        &state.title_input,
        state.focused_field == NewTaskField::Title,
        surface,
    );

    let selected = if state.ensure_base_up_to_date {
        vec![0]
    } else {
        Vec::new()
    };
    let mut checkbox = Checkbox::default()
        .title("Options", Alignment::Left)
        .borders(rounded_borders(theme.focus))
        .foreground(theme.task)
        .background(surface)
        .choices(["Ensure base is up to date"])
        .values(&selected)
        .rewind(false)
        .inactive(Style::default().fg(theme.secondary));
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
    );
    render_action_button(
        frame,
        actions[1],
        "Cancel",
        matches!(state.focused_field, NewTaskField::Cancel),
        false,
    );

    let mut hint = Label::default()
        .text("Tab/Up/Down: move focus  Enter: confirm  Esc: cancel")
        .alignment(Alignment::Center)
        .foreground(theme.secondary)
        .background(surface);
    hint.view(frame, layout[6]);
}

fn render_delete_task_dialog(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &crate::app::DeleteTaskDialogState,
) {
    let theme = Theme::default();
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

    let mut panel = Paragraph::default()
        .title("Delete Task", Alignment::Center)
        .borders(rounded_borders(theme.focus))
        .foreground(theme.task)
        .background(Color::Black)
        .text([TextSpan::from("")]);
    panel.view(frame, area);

    let mut summary = Paragraph::default()
        .foreground(theme.task)
        .background(Color::Black)
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

    let mut checkbox = Checkbox::default()
        .title("Delete Options", Alignment::Left)
        .borders(rounded_borders(theme.focus))
        .foreground(theme.task)
        .background(overlay_surface_color())
        .choices(["Kill tmux", "Remove worktree", "Delete branch"])
        .values(&selected)
        .rewind(false)
        .inactive(Style::default().fg(theme.secondary));
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
    );
    render_action_button(
        frame,
        buttons[1],
        "Cancel",
        matches!(state.focused_field, DeleteTaskField::Cancel),
        false,
    );
}

fn render_category_dialog(
    frame: &mut Frame<'_>,
    area: Rect,
    _app: &App,
    mode: CategoryInputMode,
    name: &str,
) {
    let title = match mode {
        CategoryInputMode::Add => "Add Category",
        CategoryInputMode::Rename => "Rename Category",
    };
    let header = centered_rect(100, 35, area);
    render_input_component(frame, header, title, name, true, overlay_surface_color());
}

fn render_new_project_dialog(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &crate::app::NewProjectDialogState,
) {
    render_input_component(
        frame,
        area,
        "Project Name",
        &state.name_input,
        true,
        overlay_surface_color(),
    );
}

fn render_message_dialog(frame: &mut Frame<'_>, area: Rect, title: &str, text: &str) {
    let mut paragraph = Paragraph::default()
        .title(title, Alignment::Center)
        .borders(rounded_borders(Theme::default().focus))
        .foreground(Theme::default().task)
        .background(overlay_surface_color())
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
) {
    let theme = Theme::default();
    let accent = if destructive { Color::Red } else { theme.focus };
    let fg = if focused { Color::Black } else { accent };
    let bg = if focused { accent } else { Color::Black };

    let mut button = Paragraph::default()
        .borders(rounded_borders(accent))
        .foreground(fg)
        .background(if focused { bg } else { overlay_surface_color() })
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
    let theme = Theme::default();
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
        overlay_surface_color(),
    );

    let mut hint = Label::default()
        .text("Type to filter. Enter to execute. Esc to close.")
        .alignment(Alignment::Left)
        .foreground(theme.secondary)
        .background(overlay_surface_color());
    hint.view(frame, chunks[1]);

    if !should_render_command_palette_results(app.viewport) {
        return;
    }

    let mut rows = TableBuilder::default();
    let commands = all_commands();
    for ranked in &state.filtered {
        if let Some(command) = commands.get(ranked.command_idx) {
            rows.add_col(TextSpan::from(command.display_name.to_string()))
                .add_col(TextSpan::from(command.keybinding.to_string()))
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
        .borders(rounded_borders(theme.focus))
        .foreground(theme.task)
        .highlighted_color(theme.focus)
        .highlighted_str("> ")
        .headers(["Command", "Key"])
        .widths(&[75, 25])
        .scroll(true)
        .table(rows.build())
        .selected_line(selected)
        .inactive(Style::default().fg(theme.secondary));
    list.attr(Attribute::Focus, AttrValue::Flag(true));
    list.view(frame, chunks[2]);
}

fn render_help_overlay(frame: &mut Frame<'_>) {
    let area = centered_rect(84, 84, frame.area());
    let lines = [
        "Keyboard shortcuts",
        "",
        "Global",
        "  Ctrl+P: open command palette",
        "  q: quit",
        "  ?: toggle help",
        "",
        "Board",
        "  h/l or Left/Right: move focus between columns",
        "  j/k or Up/Down: move selection",
        "  Enter: attach selected task",
        "  n: new task",
        "  c/r/x: add/rename/delete category",
        "  d: delete task",
        "  H/L: move task left/right",
        "  J/K: move task down/up",
        "  v: toggle side panel",
        "",
        "Dialogs",
        "  Enter: confirm",
        "  Esc: cancel",
    ];
    render_message_dialog(frame, area, "Help", &lines.join("\n"));
}

fn render_empty_state(frame: &mut Frame<'_>, area: Rect, message: &str) {
    let mut paragraph = Paragraph::default()
        .title("opencode-kanban", Alignment::Center)
        .borders(rounded_borders(Theme::default().secondary))
        .foreground(Theme::default().secondary)
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
) {
    let theme = Theme::default();
    let mut input = Input::default()
        .title(title, Alignment::Left)
        .borders(rounded_borders(if focused {
            theme.focus
        } else {
            theme.secondary
        }))
        .foreground(theme.task)
        .background(background)
        .inactive(Style::default().fg(theme.secondary))
        .input_type(InputType::Text)
        .value(value.to_string());
    input.attr(Attribute::Focus, AttrValue::Flag(focused));
    input.view(frame, area);
}

fn linear_entries(app: &App) -> Vec<(String, Task)> {
    let mut out = Vec::new();
    for (_, category) in sorted_categories(app) {
        let mut tasks = tasks_for_category(app, category.id);
        tasks.sort_by_key(|task| task.position);
        for task in tasks {
            out.push((category.name.clone(), task));
        }
    }
    out
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

fn rounded_borders(color: Color) -> Borders {
    Borders::default()
        .modifiers(BorderType::Rounded)
        .color(color)
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

fn overlay_surface_color() -> Color {
    Color::Rgb(36, 40, 56)
}

#[cfg(test)]
mod tests {
    use super::*;

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
