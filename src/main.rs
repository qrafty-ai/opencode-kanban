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
    tui::layout::Rect,
};

use opencode_kanban::{
    db::Database,
    logging::{init_logging, print_log_location},
    projects,
    tmux::{ensure_tmux_installed, tmux_session_exists},
    ui_realm::{
        ComponentId, application::TuiApplication, components::Footer, messages::Msg, model::Model,
    },
};

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
        .active(&ComponentId::ProjectList)
        .context("failed to set initial active component")?;

    let mut should_quit = false;
    while !should_quit {
        let messages = tui_app
            .tick(PollStrategy::UpTo(8))
            .context("failed during tui tick")?;
        should_quit =
            should_quit || update_model_from_messages(&mut model, &mut tui_app, messages)?;

        let active_component = tui_app
            .app()
            .focus()
            .copied()
            .unwrap_or(ComponentId::ProjectList);

        let bridge = guard
            .bridge
            .as_mut()
            .context("TerminalGuard already taken")?;
        bridge
            .raw_mut()
            .draw(|frame| tui_app.view(&active_component, frame, frame.size()))
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
        if matches!(&message, Msg::SelectProject(_)) {
            tui_app
                .app_mut()
                .active(&ComponentId::KanbanColumn(0))
                .context("failed to focus board after project selection")?;
        }

        if matches!(&message, Msg::ConfirmQuit)
            || matches!(&message, Msg::ExecuteCommand(command) if command == "quit")
        {
            return Ok(true);
        }

        let mut next = Some(message);
        while next.is_some() {
            next = model.update(next);
        }
    }

    Ok(false)
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
