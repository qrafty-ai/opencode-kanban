use std::{
    io, panic,
    process::Command,
    str::FromStr,
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
    cli::{self, RootCommand},
    logging::{init_logging, print_log_location},
    realm::{RootId, apply_message, init_application, should_quit},
    theme::ThemePreset,
    tmux::{ensure_tmux_installed, tmux_session_exists},
};

#[derive(Parser, Debug)]
#[command(
    name = "opencode-kanban",
    about = "Terminal kanban board for managing OpenCode tmux sessions",
    long_about = "A TUI kanban board for managing git worktrees and OpenCode sessions, orchestrated via tmux.",
    version = env!("OPENCODE_KANBAN_BUILD_VERSION"),
    author
)]
struct Cli {
    #[arg(short, long, global = true, value_name = "PROJECT")]
    project: Option<String>,

    #[arg(long, value_name = "PRESET")]
    theme: Option<String>,

    #[arg(long, global = true)]
    json: bool,

    #[arg(long)]
    quiet: bool,

    #[arg(long = "no-color")]
    no_color: bool,

    #[command(subcommand)]
    command: Option<RootCommand>,
}

enum RunOutcome {
    Continue,
    Exit(i32),
}

#[tokio::main]
async fn main() -> Result<()> {
    let log_path = init_logging().expect("Failed to initialize logging");
    install_panic_hook_with_log(log_path.clone());

    match run_app() {
        Ok(RunOutcome::Continue) => {
            print_log_location(&log_path);
            Ok(())
        }
        Ok(RunOutcome::Exit(code)) => {
            std::process::exit(code);
        }
        Err(err) => {
            print_log_location(&log_path);
            Err(err)
        }
    }
}

fn run_app() -> Result<RunOutcome> {
    let cli = Cli::parse();

    if let Some(command) = cli.command {
        let Some(project_name) = cli.project.as_deref() else {
            eprintln!("error[PROJECT_REQUIRED]: --project is required for CLI commands");
            return Ok(RunOutcome::Exit(2));
        };

        if project_name.trim().is_empty() {
            eprintln!("error[PROJECT_REQUIRED]: --project cannot be empty");
            return Ok(RunOutcome::Exit(2));
        }

        let _ = cli.no_color;
        let code = cli::run(project_name, command, cli.json, cli.quiet);
        return Ok(RunOutcome::Exit(code));
    }

    validate_runtime_environment()?;

    let mut terminal = setup_terminal()?;
    let _guard = TerminalGuard;

    let project_name = cli.project.as_deref();
    let cli_theme_override = cli
        .theme
        .as_deref()
        .and_then(|value| ThemePreset::from_str(value).ok());
    let app = Arc::new(Mutex::new(App::new_with_theme(
        project_name,
        cli_theme_override,
    )?));
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

    Ok(RunOutcome::Continue)
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
