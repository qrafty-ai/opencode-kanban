use std::env;
use std::path::Path;
use std::process::Command;
use std::sync::LazyLock;
use std::time::SystemTime;

use anyhow::{Context, Result, bail};
use regex::Regex;

use crate::tmux::{tmux_capture_pane, tmux_get_pane_pid};
use crate::types::{SessionState, SessionStatus, SessionStatusError, SessionStatusSource};

pub mod server;
pub mod status_server;

pub use crate::types::SessionState as Status;
pub use server::{OpenCodeServerManager, OpenCodeServerState, ensure_server_ready};
pub use status_server::ServerStatusProvider;

pub const DEFAULT_SERVER_URL: &str = "http://127.0.0.1:4096";

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum OpenCodeBindingState {
    Bound,
    Stale,
    Unbound,
}

pub fn classify_binding_state(
    opencode_session_id: Option<&str>,
    status: Option<&SessionStatus>,
) -> OpenCodeBindingState {
    let Some(_) = opencode_session_id else {
        return OpenCodeBindingState::Unbound;
    };

    let Some(status) = status else {
        return OpenCodeBindingState::Bound;
    };

    if status
        .error
        .as_ref()
        .map(|error| error.code.as_str())
        .is_some_and(|code| code == "SERVER_STATUS_MISSING")
    {
        return OpenCodeBindingState::Stale;
    }

    OpenCodeBindingState::Bound
}

pub trait StatusProvider {
    fn get_status(&self, session_id: &str) -> SessionStatus;

    fn list_statuses(&self, session_ids: &[String]) -> Vec<(String, SessionStatus)> {
        session_ids
            .iter()
            .map(|session_id| (session_id.clone(), self.get_status(session_id)))
            .collect()
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct TmuxStatusProvider;

impl StatusProvider for TmuxStatusProvider {
    fn get_status(&self, session_id: &str) -> SessionStatus {
        if !opencode_is_running_in_session(session_id) {
            return SessionStatus {
                state: SessionState::Dead,
                source: SessionStatusSource::Tmux,
                fetched_at: SystemTime::now(),
                error: None,
            };
        }

        match tmux_capture_pane(session_id, 50) {
            Ok(pane) => SessionStatus {
                state: opencode_detect_status(&pane),
                source: SessionStatusSource::Tmux,
                fetched_at: SystemTime::now(),
                error: None,
            },
            Err(err) => SessionStatus {
                state: SessionState::Idle,
                source: SessionStatusSource::Tmux,
                fetched_at: SystemTime::now(),
                error: Some(SessionStatusError {
                    code: "TMUX_CAPTURE_FAILED".to_string(),
                    message: err.to_string(),
                }),
            },
        }
    }
}

#[derive(Debug)]
struct PatternConfig {
    running: Regex,
    waiting: Regex,
    idle: Regex,
}

static ANSI_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\x1b\[[0-9;?]*[ -/]*[@-~]").expect("valid ansi regex"));

static UUID_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}")
        .expect("valid uuid regex")
});

static STATUS_PATTERNS: LazyLock<PatternConfig> = LazyLock::new(PatternConfig::from_env);

impl PatternConfig {
    fn from_env() -> Self {
        let running = env_or_default(
            "OPENCODE_STATUS_RUNNING_RE",
            r"(?i)(thinking|executing|processing|esc\s+to\s+interrupt|\bworking\b|\bloading\b)",
        );
        let waiting = env_or_default(
            "OPENCODE_STATUS_WAITING_RE",
            r"(?i)(press\s+enter\s+to\s+continue|continue\?\s*\[y/n\]|confirm|yes/no|allow\s+once|allow\s+always)",
        );
        let idle = env_or_default(
            "OPENCODE_STATUS_IDLE_RE",
            r"(?i)(i['’]?m\s+ready|what\s+would\s+you\s+like\s+to\s+do\?|(^|\s)>\s*$|(^|\s)\$\s*$)",
        );

        Self {
            running: Regex::new(&running).unwrap_or_else(|_| {
                Regex::new(r"(?i)(thinking|executing|processing)").expect("valid fallback running")
            }),
            waiting: Regex::new(&waiting).unwrap_or_else(|_| {
                Regex::new(r"(?i)(press\s+enter\s+to\s+continue|continue\?)")
                    .expect("valid fallback waiting")
            }),
            idle: Regex::new(&idle).unwrap_or_else(|_| {
                Regex::new(r"(?i)(i['’]?m\s+ready|what\s+would\s+you\s+like\s+to\s+do\?)")
                    .expect("valid fallback idle")
            }),
        }
    }
}

fn env_or_default(key: &str, default: &str) -> String {
    env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| default.to_string())
}

pub fn opencode_launch(working_dir: &Path, session_id: Option<String>) -> Result<String> {
    let binary = opencode_binary();
    ensure_opencode_available(&binary)?;

    let mut cmd = Command::new(&binary);
    cmd.current_dir(working_dir);

    if let Some(existing) = session_id {
        let output = cmd
            .args(["-s", existing.as_str()])
            .output()
            .with_context(|| {
                format!(
                    "failed to launch OpenCode in {} with session {}",
                    working_dir.display(),
                    existing
                )
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!(
                "failed to launch OpenCode session {existing}: {}",
                stderr.trim()
            );
        }

        return Ok(existing);
    }

    let output = cmd
        .output()
        .with_context(|| format!("failed to launch OpenCode in {}", working_dir.display()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "failed to launch OpenCode in {}: {}",
            working_dir.display(),
            stderr.trim()
        );
    }

    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    if let Some(found) = UUID_RE.find(&combined) {
        return Ok(found.as_str().to_string());
    }

    bail!(
        "OpenCode launched in {}, but no session id was found in output. Re-run with a known session id (opencode -s <session_id>) for deterministic resume.",
        working_dir.display()
    )
}

pub fn opencode_resume_session(session_id: &str, working_dir: &Path) -> Result<()> {
    let binary = opencode_binary();
    ensure_opencode_available(&binary)?;

    let output = Command::new(&binary)
        .args(["-s", session_id])
        .current_dir(working_dir)
        .output()
        .with_context(|| {
            format!(
                "failed to execute OpenCode resume in {} for session {}",
                working_dir.display(),
                session_id
            )
        })?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    bail!(
        "failed to resume OpenCode session {} in {}: {}",
        session_id,
        working_dir.display(),
        stderr.trim()
    )
}

pub fn opencode_detect_status(pane_output: &str) -> Status {
    let cleaned = strip_ansi(pane_output);
    let tail = cleaned
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(str::trim)
        .rev()
        .take(30)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join("\n");

    if tail.trim().is_empty()
        || tail.contains("connection refused")
        || tail.contains("no server running")
    {
        return Status::Dead;
    }

    if STATUS_PATTERNS.waiting.is_match(&tail) {
        return Status::Waiting;
    }
    if STATUS_PATTERNS.running.is_match(&tail) {
        return Status::Running;
    }
    if STATUS_PATTERNS.idle.is_match(&tail) {
        return Status::Idle;
    }

    Status::Idle
}

pub fn opencode_is_running_in_session(tmux_session_name: &str) -> bool {
    let Some(pane_pid) = tmux_get_pane_pid(tmux_session_name) else {
        return false;
    };

    let process_output = Command::new("ps")
        .args(["-p", &pane_pid.to_string(), "-o", "command="])
        .output();

    if let Ok(output) = process_output {
        let command_line = String::from_utf8_lossy(&output.stdout).to_lowercase();
        if command_line.contains("opencode") {
            return true;
        }
    }

    let pane = tmux_capture_pane(tmux_session_name, 50).ok();
    if let Some(content) = pane {
        return matches!(
            opencode_detect_status(&content),
            Status::Running | Status::Waiting | Status::Idle
        );
    }

    false
}

fn strip_ansi(content: &str) -> String {
    ANSI_RE.replace_all(content, "").to_string()
}

fn opencode_binary() -> String {
    env::var("OPENCODE_BIN").unwrap_or_else(|_| "opencode".to_string())
}

fn ensure_opencode_available(binary: &str) -> Result<()> {
    let output = Command::new(binary).arg("--version").output();
    match output {
        Ok(output) if output.status.success() => Ok(()),
        Ok(_) => bail!(
            "OpenCode binary is installed but not runnable (`{binary} --version` failed). Ensure OpenCode is correctly installed and accessible in this tmux environment."
        ),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => bail!(
            "OpenCode binary not found (`{binary}`). Install OpenCode and ensure it is on PATH."
        ),
        Err(err) => Err(err).with_context(|| {
            format!("failed to execute `{binary} --version` while checking OpenCode availability")
        }),
    }
}

pub fn opencode_attach_command(session_id: Option<&str>) -> String {
    let url = DEFAULT_SERVER_URL;
    match session_id {
        Some(id) => format!("opencode attach {url} --session {id}"),
        None => format!("opencode attach {url}"),
    }
}

pub fn opencode_query_session_by_dir(working_dir: &Path) -> Result<Option<String>> {
    let db_path = dirs::data_dir()
        .context("failed to determine home directory for opencode database")?
        .join("opencode")
        .join("opencode.db");

    tracing::debug!("Checking for opencode DB at: {}", db_path.display());

    if !db_path.exists() {
        tracing::debug!("OpenCode DB not found, returning None");
        return Ok(None);
    }

    let dir_str = working_dir.to_string_lossy();
    let query = format!(
        "SELECT id FROM session WHERE directory = '{}' ORDER BY time_created DESC LIMIT 1",
        dir_str.replace('\'', "''")
    );

    tracing::debug!("Querying opencode DB: {}", query);

    let output = Command::new("sqlite3")
        .args(["-batch", "-readonly", db_path.to_str().unwrap_or_default()])
        .arg(&query)
        .output()
        .with_context(|| {
            format!(
                "failed to query opencode database for directory {}",
                dir_str
            )
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        tracing::error!("SQLite query failed: {}", stderr.trim());
        bail!("sqlite3 query failed: {}", stderr.trim());
    }

    let result = String::from_utf8_lossy(&output.stdout).trim().to_string();

    if result.is_empty() {
        tracing::debug!("No session found for directory: {}", dir_str);
        return Ok(None);
    }

    tracing::debug!("Found session ID for directory {}: {}", dir_str, result);
    Ok(Some(result))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::{LazyLock, Mutex, MutexGuard};
    use std::time::SystemTime;

    use anyhow::Result;
    use uuid::Uuid;

    use super::*;

    static TEST_ENV_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

    #[test]
    fn test_detect_running_status() {
        let pane = "OpenCode\nthinking about refactor\nexecuting tool call";
        assert_eq!(opencode_detect_status(pane), Status::Running);
    }

    #[test]
    fn test_detect_waiting_status() {
        let pane = "Need confirmation\nContinue? [y/n]";
        assert_eq!(opencode_detect_status(pane), Status::Waiting);
    }

    #[test]
    fn test_detect_idle_status() {
        let pane = "I'm ready\nWhat would you like to do?";
        assert_eq!(opencode_detect_status(pane), Status::Idle);
    }

    #[test]
    fn test_detect_dead_status_empty_pane() {
        assert_eq!(opencode_detect_status("\n\n"), Status::Dead);
    }

    #[test]
    fn test_detect_status_strips_ansi_sequences() {
        let pane = "\x1b[32mthinking\x1b[0m";
        assert_eq!(opencode_detect_status(pane), Status::Running);
    }

    #[test]
    fn test_detect_status_waiting_precedes_running() {
        let pane = "thinking about next action\nContinue? [y/n]";
        assert_eq!(opencode_detect_status(pane), Status::Waiting);
    }

    #[test]
    fn test_status_provider_list_statuses_preserves_order_and_metadata() {
        let provider = StaticStatusProvider {
            statuses: HashMap::from([
                (
                    "a".to_string(),
                    SessionStatus {
                        state: Status::Running,
                        source: SessionStatusSource::Server,
                        fetched_at: SystemTime::UNIX_EPOCH,
                        error: None,
                    },
                ),
                (
                    "b".to_string(),
                    SessionStatus {
                        state: Status::Dead,
                        source: SessionStatusSource::Tmux,
                        fetched_at: SystemTime::UNIX_EPOCH,
                        error: Some(SessionStatusError {
                            code: "TEST".to_string(),
                            message: "boom".to_string(),
                        }),
                    },
                ),
            ]),
        };

        let listed = provider.list_statuses(&["b".to_string(), "a".to_string()]);
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].0, "b");
        assert_eq!(listed[0].1.state, Status::Dead);
        assert_eq!(listed[0].1.source, SessionStatusSource::Tmux);
        assert_eq!(
            listed[0].1.error.as_ref().map(|err| err.code.as_str()),
            Some("TEST")
        );
        assert_eq!(listed[1].0, "a");
        assert_eq!(listed[1].1.state, Status::Running);
        assert_eq!(listed[1].1.source, SessionStatusSource::Server);
    }

    #[test]
    fn test_classify_binding_state_unbound_without_session_id() {
        assert_eq!(
            classify_binding_state(None, None),
            OpenCodeBindingState::Unbound
        );
    }

    #[test]
    fn test_classify_binding_state_stale_when_server_reports_missing() {
        let status = SessionStatus {
            state: Status::Idle,
            source: SessionStatusSource::None,
            fetched_at: SystemTime::UNIX_EPOCH,
            error: Some(SessionStatusError {
                code: "SERVER_STATUS_MISSING".to_string(),
                message: "missing".to_string(),
            }),
        };

        assert_eq!(
            classify_binding_state(Some("sid-1"), Some(&status)),
            OpenCodeBindingState::Stale
        );
    }

    #[test]
    fn test_classify_binding_state_bound_on_non_definitive_errors() {
        let status = SessionStatus {
            state: Status::Idle,
            source: SessionStatusSource::None,
            fetched_at: SystemTime::UNIX_EPOCH,
            error: Some(SessionStatusError {
                code: "SERVER_TIMEOUT".to_string(),
                message: "timeout".to_string(),
            }),
        };

        assert_eq!(
            classify_binding_state(Some("sid-1"), Some(&status)),
            OpenCodeBindingState::Bound
        );
    }

    #[test]
    fn test_classify_binding_state_bound_when_server_is_down() {
        let status = SessionStatus {
            state: Status::Idle,
            source: SessionStatusSource::None,
            fetched_at: SystemTime::UNIX_EPOCH,
            error: Some(SessionStatusError {
                code: "SERVER_CONNECT_FAILED".to_string(),
                message: "connection refused".to_string(),
            }),
        };

        assert_eq!(
            classify_binding_state(Some("sid-1"), Some(&status)),
            OpenCodeBindingState::Bound
        );
    }

    #[test]
    fn test_classify_binding_state_bound_on_server_auth_failure() {
        let status = SessionStatus {
            state: Status::Idle,
            source: SessionStatusSource::None,
            fetched_at: SystemTime::UNIX_EPOCH,
            error: Some(SessionStatusError {
                code: "SERVER_AUTH_ERROR".to_string(),
                message: "unauthorized".to_string(),
            }),
        };

        assert_eq!(
            classify_binding_state(Some("sid-1"), Some(&status)),
            OpenCodeBindingState::Bound
        );
    }

    #[test]
    fn test_classify_binding_state_bound_on_parse_contract_mismatch() {
        let status = SessionStatus {
            state: Status::Idle,
            source: SessionStatusSource::None,
            fetched_at: SystemTime::UNIX_EPOCH,
            error: Some(SessionStatusError {
                code: "SERVER_CONTRACT_PARSE_ERROR".to_string(),
                message: "invalid response contract".to_string(),
            }),
        };

        assert_eq!(
            classify_binding_state(Some("sid-1"), Some(&status)),
            OpenCodeBindingState::Bound
        );
    }

    #[test]
    fn test_launch_with_existing_session_uses_resume_arg() -> Result<()> {
        let fixture = FakeOpenCode::new("with-session")?;
        let session_id = Uuid::new_v4().to_string();

        let returned = opencode_launch(fixture.temp.path(), Some(session_id.clone()))?;
        assert_eq!(returned, session_id);

        let logged = fs::read_to_string(fixture.log_path())?;
        assert!(logged.contains("ARGS: -s"));
        assert!(logged.contains(&session_id));

        Ok(())
    }

    #[test]
    fn test_launch_without_session_extracts_uuid() -> Result<()> {
        let fixture = FakeOpenCode::new("new-session")?;
        let generated = opencode_launch(fixture.temp.path(), None)?;
        assert!(Uuid::parse_str(&generated).is_ok());
        Ok(())
    }

    #[test]
    fn test_resume_session_runs_with_context() -> Result<()> {
        let fixture = FakeOpenCode::new("resume")?;
        let sid = Uuid::new_v4().to_string();
        opencode_resume_session(&sid, fixture.temp.path())?;

        let logged = fs::read_to_string(fixture.log_path())?;
        assert!(logged.contains("ARGS: -s"));
        assert!(logged.contains(&sid));

        Ok(())
    }

    struct FakeOpenCode {
        temp: tempfile::TempDir,
        _guard: MutexGuard<'static, ()>,
    }

    impl FakeOpenCode {
        fn new(name: &str) -> Result<Self> {
            let guard = TEST_ENV_LOCK.lock().expect("test env mutex should lock");
            let temp = tempfile::Builder::new().prefix(name).tempdir()?;
            let script_path = temp.path().join("fake-opencode");
            let log_path = temp.path().join("calls.log");

            let script = format!(
                "#!/usr/bin/env bash\nset -euo pipefail\nif [[ \"${{1:-}}\" == \"--version\" ]]; then\n  printf 'opencode 0.0.0\\n'\n  exit 0\nfi\nprintf 'ARGS: %s\\n' \"$*\" >> \"{}\"\nif [[ \"${{1:-}}\" == \"-s\" ]]; then\n  printf 'Resumed %s\\n' \"${{2:-missing}}\"\n  exit 0\nfi\npython3 - <<'PY'\nimport uuid\nprint(str(uuid.uuid4()))\nPY\n",
                log_path.display()
            );

            fs::write(&script_path, script)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&script_path, fs::Permissions::from_mode(0o755))?;
            }

            unsafe {
                env::set_var("OPENCODE_BIN", script_path.to_string_lossy().to_string());
            }

            Ok(Self {
                temp,
                _guard: guard,
            })
        }

        fn log_path(&self) -> PathBuf {
            self.temp.path().join("calls.log")
        }
    }

    impl Drop for FakeOpenCode {
        fn drop(&mut self) {
            unsafe {
                env::remove_var("OPENCODE_BIN");
            }
        }
    }

    struct StaticStatusProvider {
        statuses: HashMap<String, SessionStatus>,
    }

    impl StatusProvider for StaticStatusProvider {
        fn get_status(&self, session_id: &str) -> SessionStatus {
            self.statuses
                .get(session_id)
                .cloned()
                .unwrap_or(SessionStatus {
                    state: Status::Idle,
                    source: SessionStatusSource::None,
                    fetched_at: SystemTime::UNIX_EPOCH,
                    error: None,
                })
        }
    }
}
