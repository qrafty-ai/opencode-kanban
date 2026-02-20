use std::fs;
use std::path::{Path, PathBuf};

use chrono::Local;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

const LOG_LEVEL_ENV: &str = "OPENCODE_KANBAN_LOG_LEVEL";

pub fn init_logging() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let log_dir = get_log_directory()?;
    fs::create_dir_all(&log_dir)?;

    let log_file_path = get_log_file_path(&log_dir);

    let file = fs::File::create(&log_file_path)?;
    let (non_blocking, guard) = tracing_appender::non_blocking(file);

    std::mem::forget(guard);

    let env_filter = build_log_filter();

    let file_layer = fmt::layer()
        .with_writer(non_blocking)
        .with_ansi(false)
        .with_target(true)
        .with_thread_ids(true)
        .with_file(true)
        .with_line_number(true);

    tracing_subscriber::registry()
        .with(env_filter)
        .with(file_layer)
        .init();

    tracing::info!("Logging initialized. Log file: {}", log_file_path.display());

    Ok(log_file_path)
}

fn build_log_filter() -> EnvFilter {
    let default_level = "warn";
    let level = std::env::var(LOG_LEVEL_ENV)
        .ok()
        .and_then(|raw| normalize_log_level(raw.as_str()))
        .unwrap_or(default_level);
    EnvFilter::new(format!("{level},opencode_kanban={level}"))
}

fn normalize_log_level(raw: &str) -> Option<&'static str> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "trace" => Some("trace"),
        "debug" => Some("debug"),
        "info" => Some("info"),
        "warn" | "warning" => Some("warn"),
        "error" => Some("error"),
        _ => None,
    }
}

pub fn get_log_directory() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let data_dir = dirs::data_local_dir().ok_or("Failed to determine local data directory")?;
    Ok(data_dir.join("opencode-kanban").join("logs"))
}

pub fn get_log_file_path(log_dir: &Path) -> PathBuf {
    let timestamp = Local::now().format("%Y-%m-%d_%H-%M-%S").to_string();
    log_dir.join(format!("opencode-kanban-{}.log", timestamp))
}

pub fn print_log_location(log_path: &Path) {
    println!();
    println!("═══════════════════════════════════════════════════════════════");
    println!("  📝 Log file: {}", log_path.display());
    println!("═══════════════════════════════════════════════════════════════");
    println!();
}

pub fn get_recent_log_path() -> Option<PathBuf> {
    let log_dir = get_log_directory().ok()?;

    if !log_dir.exists() {
        return None;
    }

    let mut entries = fs::read_dir(&log_dir)
        .ok()?
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.starts_with("opencode-kanban-") && n.ends_with(".log"))
                .unwrap_or(false)
        })
        .collect::<Vec<_>>();

    entries.sort_by_key(|e| std::cmp::Reverse(e.metadata().ok().and_then(|m| m.modified().ok())));

    entries.into_iter().next().map(|e| e.path())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_log_directory() {
        let dir = get_log_directory();
        assert!(dir.is_ok());
        let path = dir.unwrap();
        assert!(path.to_string_lossy().contains("opencode-kanban"));
    }

    #[test]
    fn test_get_log_file_path() {
        let dir = PathBuf::from("/tmp/test-logs");
        let path = get_log_file_path(&dir);
        let path_str = path.to_string_lossy();
        assert!(path_str.contains("opencode-kanban-"));
        assert!(path_str.ends_with(".log"));
    }

    #[test]
    fn test_normalize_log_level() {
        assert_eq!(normalize_log_level("TRACE"), Some("trace"));
        assert_eq!(normalize_log_level("warning"), Some("warn"));
        assert_eq!(normalize_log_level("nope"), None);
    }
}
