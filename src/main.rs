use std::{io, panic, process::Command, time::Duration};

use anyhow::{Context, Result, bail};
use clap::Parser;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};

use opencode_kanban::{
    app::App,
    input::event_to_message,
    logging::{init_logging, print_log_location},
    tmux::{ensure_tmux_installed, tmux_session_exists},
    ui,
};

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

    let mut terminal = setup_terminal()?;
    let _guard = TerminalGuard;

    let project_name = cli.project.as_deref();
    let mut app = App::new(project_name)?;

    while !app.should_quit() {
        terminal
            .draw(|frame| ui::render(frame, &mut app))
            .context("failed to render frame")?;

        if event::poll(Duration::from_millis(100)).context("failed to poll events")? {
            let event = event::read().context("failed to read terminal event")?;
            if let Some(message) = event_to_message(event) {
                app.update(message)?;
            }
        }
    }

    Ok(())
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

fn setup_terminal() -> Result<Terminal<CrosstermBackend<io::Stdout>>> {
    enable_raw_mode().context("failed to enable raw mode")?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)
        .context("failed to enter alternate screen")?;

    let backend = CrosstermBackend::new(stdout);
    Terminal::new(backend).context("failed to create terminal")
}

fn install_panic_hook_with_log(log_path: std::path::PathBuf) {
    let previous_hook = panic::take_hook();
    panic::set_hook(Box::new(move |panic_info| {
        let _ = restore_terminal();
        eprintln!();
        eprintln!("═══════════════════════════════════════════════════════════════");
        eprintln!("  📝 Log file: {}", log_path.display());
        eprintln!("═══════════════════════════════════════════════════════════════");
        eprintln!();
        previous_hook(panic_info);
    }));
}

fn restore_terminal() -> Result<()> {
    disable_raw_mode().context("failed to disable raw mode")?;
    let mut stdout = io::stdout();
    execute!(stdout, LeaveAlternateScreen, DisableMouseCapture)
        .context("failed to leave alternate screen")?;
    Ok(())
}

struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = restore_terminal();
    }
}
