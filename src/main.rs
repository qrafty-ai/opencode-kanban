use std::{panic, path::PathBuf, process::Command, time::Duration};

use anyhow::{Context, Result, bail};
use clap::Parser;
use tuirealm::terminal::TerminalBridge;
use tuirealm::{
    AttrValue, Attribute, Component, Event, Frame, MockComponent, NoUserEvent, PollStrategy, State,
    Sub, SubClause, SubEventClause, Update,
    command::{Cmd, CmdResult},
    event::{Key, KeyEvent, KeyModifiers},
    listener::EventListenerCfg,
    tui::{
        layout::{Alignment, Constraint, Direction, Layout, Rect},
        style::{Color, Style},
        text::Span,
        widgets::{Block, Borders},
    },
};

use opencode_kanban::{
    db::Database,
    logging::{init_logging, print_log_location},
    projects,
    tmux::{ensure_tmux_installed, tmux_session_exists},
    ui_realm::{
        ComponentId,
        application::TuiApplication,
        components::{ErrorDialog, ErrorDialogVariant, Footer},
        messages::Msg,
        model::Model,
    },
};

const BOARD_MOUSE_HINT: &str =
    " tmux mouse hint: run `tmux set -g mouse on` for click+scroll support ";
const BOARD_AUTO_REFRESH: &str = "auto-refresh: 0.5s";
const BOARD_TITLE: &str = " opencode-kanban ";

struct GlobalHotkeysFooter {
    inner: Footer,
}

impl GlobalHotkeysFooter {
    fn new() -> Self {
        Self {
            inner: Footer::new(),
        }
    }
}

impl MockComponent for GlobalHotkeysFooter {
    fn view(&mut self, frame: &mut Frame, area: Rect) {
        self.inner.view(frame, area);
    }

    fn query(&self, attr: Attribute) -> Option<AttrValue> {
        self.inner.query(attr)
    }

    fn attr(&mut self, attr: Attribute, value: AttrValue) {
        self.inner.attr(attr, value);
    }

    fn state(&self) -> State {
        self.inner.state()
    }

    fn perform(&mut self, cmd: Cmd) -> CmdResult {
        self.inner.perform(cmd)
    }
}

impl Component<Msg, NoUserEvent> for GlobalHotkeysFooter {
    fn on(&mut self, ev: Event<NoUserEvent>) -> Option<Msg> {
        match ev {
            Event::Keyboard(KeyEvent {
                code: Key::Char('q'),
                ..
            })
            | Event::Keyboard(KeyEvent {
                code: Key::Char('Q'),
                ..
            }) => Some(Msg::ConfirmQuit),
            Event::Keyboard(KeyEvent {
                code: Key::Char('c'),
                modifiers,
            }) if modifiers.contains(KeyModifiers::CONTROL) => Some(Msg::ConfirmQuit),
            _ => self.inner.on(ev),
        }
    }
}

/// RAII guard for TerminalBridge cleanup.
/// Ensures terminal is restored even on early error returns.
struct TerminalGuard {
    bridge: Option<TerminalBridge>,
}

impl TerminalGuard {
    fn new() -> Result<Self> {
        let bridge = TerminalBridge::new().context("failed to initialize terminal")?;
        Ok(Self {
            bridge: Some(bridge),
        })
    }

    fn initialize(&mut self) -> Result<()> {
        let bridge = self
            .bridge
            .as_mut()
            .context("TerminalGuard already taken")?;
        bridge
            .enable_raw_mode()
            .context("failed to enable raw mode")?;
        bridge
            .enter_alternate_screen()
            .context("failed to enter alternate screen")?;
        Ok(())
    }

    fn take(&mut self) -> TerminalBridge {
        self.bridge.take().expect("TerminalGuard already taken")
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        if let Some(mut bridge) = self.bridge.take() {
            let _ = bridge.leave_alternate_screen();
            let _ = bridge.disable_raw_mode();
        }
    }
}

#[derive(Parser, Debug)]
#[command(
    name = "opencode-kanban",
    about = "Terminal kanban board for managing OpenCode tmux sessions",
    long_about = "A TUI kanban board for managing git worktrees and OpenCode sessions, orchestrated via tmux.",
    version,
    author
)]
struct Cli {
    #[arg(short, long, value_name = "PROJECT")]
    project: Option<String>,
}

fn main() -> Result<()> {
    let log_path = init_logging().expect("Failed to initialize logging");
    install_panic_hook_with_log(log_path.clone());

    let result = run_app();

    print_log_location(&log_path);

    result
}

fn run_app() -> Result<()> {
    validate_runtime_environment()?;

    let cli = Cli::parse();

    // RAII guard ensures terminal cleanup on all error paths
    let mut guard = TerminalGuard::new()?;
    guard.initialize()?;

    let model_path = resolve_model_db_path(cli.project.as_deref())?;
    let db = Database::open(&model_path).context("failed to open model database")?;
    let mut model = Model::new(db).context("failed to initialize model")?;

    let listener_cfg = EventListenerCfg::default()
        .default_input_listener(Duration::from_millis(16))
        .poll_timeout(Duration::from_millis(16))
        .tick_interval(Duration::from_millis(250));
    let mut tui_app = TuiApplication::with_listener(listener_cfg);
    tui_app
        .wire_components(&model)
        .context("failed to wire tui-realm components")?;
    tui_app
        .app_mut()
        .remount(
            ComponentId::Footer,
            Box::new(GlobalHotkeysFooter::new()),
            vec![
                Sub::new(
                    SubEventClause::Keyboard(KeyEvent::from(Key::Char('q'))),
                    SubClause::Always,
                ),
                Sub::new(
                    SubEventClause::Keyboard(KeyEvent::from(Key::Char('Q'))),
                    SubClause::Always,
                ),
                Sub::new(
                    SubEventClause::Keyboard(KeyEvent::new(Key::Char('c'), KeyModifiers::CONTROL)),
                    SubClause::Always,
                ),
            ],
        )
        .context("failed to register global hotkeys")?;
    tui_app
        .app_mut()
        .attr(
            &ComponentId::Footer,
            Attribute::Text,
            AttrValue::String(BOARD_MOUSE_HINT.to_string()),
        )
        .context("failed to set footer hint")?;
    let initial_focus = board_focus_component(&model);
    tui_app
        .app_mut()
        .active(&initial_focus)
        .context("failed to set initial active component")?;

    let mut should_quit = false;
    while !should_quit {
        let messages = tui_app
            .tick(PollStrategy::UpTo(8))
            .context("failed during tui tick")?;
        should_quit =
            should_quit || update_model_from_messages(&mut model, &mut tui_app, messages)?;

        let bridge = guard
            .bridge
            .as_mut()
            .context("TerminalGuard already taken")?;
        bridge
            .raw_mut()
            .draw(|frame| render_ui(frame, &mut tui_app, &model))
            .context("failed to render frame")?;
    }

    // Explicit cleanup via guard (RAII will also cleanup on any error)
    let mut bridge = guard.take();
    bridge
        .leave_alternate_screen()
        .context("failed to leave alternate screen")?;
    bridge
        .disable_raw_mode()
        .context("failed to disable raw mode")?;

    Ok(())
}

fn resolve_model_db_path(project_name: Option<&str>) -> Result<PathBuf> {
    if let Some(project_name) = project_name {
        let project = projects::list_projects()?
            .into_iter()
            .find(|project| project.name == project_name)
            .with_context(|| format!("project '{}' not found", project_name))?;
        return Ok(project.path);
    }

    let path = projects::get_project_path(projects::DEFAULT_PROJECT);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create data dir {}", parent.display()))?;
    }
    Ok(path)
}

fn update_model_from_messages(
    model: &mut Model,
    tui_app: &mut TuiApplication,
    messages: Vec<Msg>,
) -> Result<bool> {
    for message in messages {
        let mut pending = std::collections::VecDeque::from([message]);
        while let Some(current) = pending.pop_front() {
            let _ = tui_app
                .handle_resize(model, &current)
                .context("failed to handle resize event")?;

            if matches!(&current, Msg::ConfirmQuit)
                || matches!(&current, Msg::ExecuteCommand(command) if command == "quit")
            {
                return Ok(true);
            }

            if let Some(follow_up) = model.update(Some(current.clone())) {
                pending.push_back(follow_up);
            }

            if let Msg::ShowError(detail) = &current {
                tui_app
                    .app_mut()
                    .remount(
                        ComponentId::Error,
                        Box::new(ErrorDialog::new(ErrorDialogVariant::Generic {
                            title: "Error".to_string(),
                            detail: detail.clone(),
                        })),
                        vec![],
                    )
                    .context("failed to update error dialog content")?;
            }

            if matches!(
                &current,
                Msg::CreateTask
                    | Msg::ConfirmDeleteTask
                    | Msg::MoveTaskLeft
                    | Msg::MoveTaskRight
                    | Msg::MoveTaskUp
                    | Msg::MoveTaskDown
                    | Msg::AttachTask
            ) {
                tui_app
                    .wire_components(model)
                    .context("failed to refresh components after model update")?;
                let focus_target = board_focus_component(model);
                if tui_app.app().mounted(&focus_target) {
                    tui_app
                        .app_mut()
                        .active(&focus_target)
                        .context("failed to restore board focus after model update")?;
                }
            }

            if matches!(&current, Msg::OpenProjectList) {
                tui_app
                    .app_mut()
                    .active(&ComponentId::ProjectList)
                    .context("failed to focus project list")?;
            }

            if matches!(&current, Msg::SelectProject(_)) {
                let focus_target = board_focus_component(model);
                tui_app
                    .app_mut()
                    .active(&focus_target)
                    .context("failed to focus board after project selection")?;
            }

            if let Msg::FocusColumn(index) = &current {
                let focus_target = ComponentId::KanbanColumn(*index);
                if tui_app.app().mounted(&focus_target) {
                    tui_app
                        .app_mut()
                        .active(&focus_target)
                        .context("failed to focus selected board column")?;
                }
            }

            let _ = tui_app
                .route_modal_focus(model, &current)
                .context("failed to route modal focus")?;
        }
    }

    Ok(false)
}

fn render_ui(frame: &mut Frame<'_>, tui_app: &mut TuiApplication, model: &Model) {
    let active_component = tui_app
        .app()
        .focus()
        .copied()
        .unwrap_or_else(|| board_focus_component(model));

    if active_component == ComponentId::ProjectList {
        tui_app.view(&ComponentId::ProjectList, frame, frame.size());
        return;
    }

    let area = frame.size();
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(area);

    render_board_header(frame, sections[0], model.tasks.len());
    render_board_columns(frame, tui_app, model, sections[1]);
    tui_app.view(&ComponentId::Footer, frame, sections[2]);

    if is_modal_component(active_component) {
        let popup = centered_rect(70, 70, area);
        tui_app.view(&active_component, frame, popup);
    }
}

fn render_board_header(frame: &mut Frame<'_>, area: Rect, task_count: usize) {
    let style = Style::default().fg(Color::Blue);
    let header = Block::default()
        .borders(Borders::TOP | Borders::LEFT | Borders::RIGHT)
        .border_style(style)
        .title(Span::styled(BOARD_TITLE, style))
        .title_alignment(Alignment::Left);
    let right_title = Block::default()
        .title(Span::styled(
            format!(" {task_count} tasks - {BOARD_AUTO_REFRESH} "),
            style,
        ))
        .title_alignment(Alignment::Right);

    frame.render_widget(header, area);
    frame.render_widget(right_title, area);
}

fn render_board_columns(
    frame: &mut Frame<'_>,
    tui_app: &mut TuiApplication,
    model: &Model,
    area: Rect,
) {
    let column_count = model.categories.len().max(1);
    let constraints = vec![Constraint::Ratio(1, column_count as u32); column_count];
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(constraints)
        .split(area);

    if model.categories.is_empty() {
        tui_app.view(&ComponentId::KanbanColumn(0), frame, columns[0]);
        return;
    }

    for (index, chunk) in columns.iter().enumerate().take(model.categories.len()) {
        tui_app.view(&ComponentId::KanbanColumn(index), frame, *chunk);
    }
}

fn board_focus_component(model: &Model) -> ComponentId {
    if model.categories.is_empty() {
        ComponentId::KanbanColumn(0)
    } else {
        ComponentId::KanbanColumn(model.focused_category.min(model.categories.len() - 1))
    }
}

fn centered_rect(width_percent: u16, height_percent: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - height_percent) / 2),
            Constraint::Percentage(height_percent),
            Constraint::Percentage((100 - height_percent) / 2),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - width_percent) / 2),
            Constraint::Percentage(width_percent),
            Constraint::Percentage((100 - width_percent) / 2),
        ])
        .split(vertical[1])[1]
}

fn is_modal_component(component_id: ComponentId) -> bool {
    matches!(
        component_id,
        ComponentId::CommandPalette
            | ComponentId::NewTask
            | ComponentId::DeleteTask
            | ComponentId::CategoryInput
            | ComponentId::DeleteCategory
            | ComponentId::NewProject
            | ComponentId::ConfirmQuit
            | ComponentId::Help
            | ComponentId::WorktreeNotFound
            | ComponentId::RepoUnavailable
            | ComponentId::Error
            | ComponentId::MoveTask
    )
}

fn validate_runtime_environment() -> Result<()> {
    if !cfg!(target_os = "linux") && !cfg!(target_os = "macos") {
        bail!("opencode-kanban supports only Linux and macOS.");
    }

    ensure_tmux_installed()?;

    if std::env::var_os("TMUX").is_none() {
        let session_name = "opencode-kanban";
        let current_exe = std::env::current_exe().context("failed to get current executable")?;
        let exe_path = current_exe.to_string_lossy();

        if tmux_session_exists(session_name) {
            let mut child = Command::new("tmux")
                .args(["attach-session", "-t", session_name])
                .spawn()
                .context("failed to attach to tmux session")?;

            child.wait().context("tmux attach-session failed")?;
            std::process::exit(0);
        }

        let mut child = Command::new("tmux")
            .args([
                "new-session",
                "-A",
                "-s",
                session_name,
                "-c",
                ".",
                exe_path.as_ref(),
            ])
            .spawn()
            .context("failed to create tmux session")?;

        child.wait().context("tmux new-session failed")?;
        std::process::exit(0);
    }

    Ok(())
}

fn install_panic_hook_with_log(log_path: std::path::PathBuf) {
    let previous_hook = panic::take_hook();
    panic::set_hook(Box::new(move |panic_info| {
        eprintln!();
        eprintln!("═══════════════════════════════════════════════════════════════");
        eprintln!("  📝 Log file: {}", log_path.display());
        eprintln!("═══════════════════════════════════════════════════════════════");
        eprintln!();
        previous_hook(panic_info);
    }));
}
