use std::{
    io, panic,
    process::Command,
    sync::{Arc, Mutex},
};

use anyhow::{Context, Result, bail};
use clap::Parser;
use crossterm::{
    event::DisableMouseCapture,
    execute,
    terminal::{LeaveAlternateScreen, disable_raw_mode},
};
use tuirealm::{
    PollStrategy,
    terminal::{CrosstermTerminalAdapter, TerminalBridge},
};

use opencode_kanban::{
    app::App,
    logging::{init_logging, print_log_location},
    realm::{RootId, apply_message, init_application, should_quit},
    tmux::{ensure_tmux_installed, tmux_session_exists},
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
    let app = Arc::new(Mutex::new(App::new(project_name)?));
    let mut realm = init_application(Arc::clone(&app))?;

    let mut redraw = true;
    while !should_quit(&app)? {
        if redraw {
            terminal
                .draw(|frame| realm.view(&RootId::Root, frame, frame.area()))
                .context("failed to render frame")?;
            redraw = false;
        }

        let messages = realm
            .tick(PollStrategy::Once)
            .context("failed to process tui-realm tick")?;

        if !messages.is_empty() {
            redraw = true;
        }

        for message in messages {
            apply_message(&app, message)?;
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

fn setup_terminal() -> Result<TerminalBridge<CrosstermTerminalAdapter>> {
    let mut terminal =
        TerminalBridge::init_crossterm().context("failed to initialize terminal bridge")?;
    terminal
        .enable_mouse_capture()
        .context("failed to enable mouse capture")?;

    Ok(terminal)
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
