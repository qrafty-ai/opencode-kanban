use std::{io, panic, time::Duration};

use anyhow::{Context, Result, bail};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};

use opencode_kanban::{app::App, input::event_to_message, tmux::ensure_tmux_installed, ui};

fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    validate_runtime_environment()?;
    install_panic_hook();

    let mut terminal = setup_terminal()?;
    let _guard = TerminalGuard;

    let mut app = App::new()?;

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

    if std::env::var_os("TMUX").is_none() {
        bail!(
            "opencode-kanban must run inside tmux. Start tmux first, then run `opencode-kanban`."
        );
    }

    ensure_tmux_installed()?;

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

fn install_panic_hook() {
    let previous_hook = panic::take_hook();
    panic::set_hook(Box::new(move |panic_info| {
        let _ = restore_terminal();
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
