use std::env;
use std::path::Path;
use std::process::Command;
use std::sync::LazyLock;

use anyhow::{Context, Result, anyhow, bail};
use regex::Regex;
use reqwest::StatusCode;
use reqwest::blocking::Client;
use serde::Deserialize;
use urlencoding::encode;

use crate::tmux::tmux_get_pane_pid;
use crate::types::SessionStatus;

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
    // Check for definitive missing-session errors first, regardless of session id.
    if let Some(code) = status
        .and_then(|s| s.error.as_ref())
        .map(|e| e.code.as_str())
        && (code == "SERVER_STATUS_MISSING" || code == "SESSION_NOT_FOUND")
    {
        return OpenCodeBindingState::Stale;
    }

    let Some(_) = opencode_session_id else {
        return OpenCodeBindingState::Unbound;
    };

    if status.is_none() {
        return OpenCodeBindingState::Bound;
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

static UUID_RE: LazyLock<std::result::Result<Regex, regex::Error>> = LazyLock::new(|| {
    Regex::new(r"[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}")
});

fn session_id_regex() -> Result<&'static Regex> {
    UUID_RE
        .as_ref()
        .map_err(|err| anyhow!("failed to compile OpenCode session-id matcher regex: {err}"))
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
    let uuid_re = session_id_regex().context("failed to initialize OpenCode session-id matcher")?;
    if let Some(found) = uuid_re.find(&combined) {
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

pub fn opencode_is_running_in_session(tmux_session_name: &str) -> bool {
    let Some(pane_pid) = tmux_get_pane_pid(tmux_session_name) else {
        return false;
    };

    let process_output = Command::new("ps")
        .args(["-p", &pane_pid.to_string(), "-o", "command="])
        .output();

    if let Ok(output) = process_output {
        let command_line = String::from_utf8_lossy(&output.stdout).to_lowercase();
        return command_line.contains("opencode");
    }

    false
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

pub fn opencode_attach_command(session_id: Option<&str>, worktree_dir: Option<&str>) -> String {
    let mut parts = vec![format!("opencode attach {DEFAULT_SERVER_URL}")];

    if let Some(dir) = worktree_dir {
        parts.push(format!("--dir {dir}"));
    }
    if let Some(id) = session_id {
        parts.push(format!("--session {id}"));
    }

    parts.join(" ")
}

pub fn opencode_query_session_by_dir(working_dir: &Path) -> Result<Option<String>> {
    #[derive(Debug, Deserialize)]
    struct SessionListEntry {
        #[serde(default)]
        id: String,
    }

    let config = status_server::ServerStatusConfig::default();
    let dir_str = working_dir.to_string_lossy();
    let session_url = format!(
        "http://{}:{}/session?directory={}",
        config.hostname,
        config.port,
        encode(&dir_str)
    );

    tracing::debug!("Querying OpenCode session API: {session_url}");

    let client = Client::builder()
        .timeout(config.request_timeout)
        .build()
        .context("failed to build OpenCode session lookup client")?;

    let response = client
        .get(&session_url)
        .send()
        .with_context(|| format!("failed to query OpenCode sessions for directory {dir_str}"))?;

    let status = response.status();
    if status == StatusCode::UNAUTHORIZED {
        bail!("OpenCode server rejected session lookup with HTTP 401");
    }
    if status != StatusCode::OK {
        bail!("OpenCode server returned HTTP {status} for /session");
    }

    let body = response
        .text()
        .context("failed to read OpenCode /session response body")?;
    let sessions: Vec<SessionListEntry> =
        serde_json::from_str(&body).context("failed to parse OpenCode /session response JSON")?;

    let session_id = sessions
        .into_iter()
        .find_map(|entry| match entry.id.trim() {
            "" => None,
            id => Some(id.to_string()),
        });

    if let Some(found) = &session_id {
        tracing::debug!("Found session ID for directory {dir_str}: {found}");
    } else {
        tracing::debug!("No OpenCode session found for directory: {dir_str}");
    }

    Ok(session_id)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::fs;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::path::PathBuf;
    use std::sync::{LazyLock, Mutex, MutexGuard};
    use std::thread;
    use std::time::SystemTime;

    use anyhow::Result;
    use uuid::Uuid;

    use super::*;
    use crate::types::{SessionStatusError, SessionStatusSource};

    static TEST_ENV_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

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
                        state: Status::Idle,
                        source: SessionStatusSource::Server,
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
        assert_eq!(listed[0].1.state, Status::Idle);
        assert_eq!(listed[0].1.source, SessionStatusSource::Server);
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
    fn test_classify_binding_state_stale_when_missing_status_and_no_session_id() {
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
            classify_binding_state(None, Some(&status)),
            OpenCodeBindingState::Stale
        );
    }

    #[test]
    fn test_classify_binding_state_stale_on_session_not_found_no_session_id() {
        let status = SessionStatus {
            state: Status::Idle,
            source: SessionStatusSource::None,
            fetched_at: SystemTime::UNIX_EPOCH,
            error: Some(SessionStatusError {
                code: "SESSION_NOT_FOUND".to_string(),
                message: "not found".to_string(),
            }),
        };

        assert_eq!(
            classify_binding_state(None, Some(&status)),
            OpenCodeBindingState::Stale
        );
    }

    #[test]
    fn test_classify_binding_state_stale_on_session_not_found_with_session_id() {
        let status = SessionStatus {
            state: Status::Idle,
            source: SessionStatusSource::None,
            fetched_at: SystemTime::UNIX_EPOCH,
            error: Some(SessionStatusError {
                code: "SESSION_NOT_FOUND".to_string(),
                message: "not found".to_string(),
            }),
        };

        assert_eq!(
            classify_binding_state(Some("sid-1"), Some(&status)),
            OpenCodeBindingState::Stale
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

    #[test]
    fn test_attach_command_without_session_includes_dir() {
        let command = opencode_attach_command(None, Some("/tmp/worktree"));
        assert_eq!(
            command,
            "opencode attach http://127.0.0.1:4096 --dir /tmp/worktree"
        );
    }

    #[test]
    fn test_attach_command_with_session_includes_dir() {
        let command = opencode_attach_command(Some("sid-123"), Some("/tmp/worktree"));
        assert_eq!(
            command,
            "opencode attach http://127.0.0.1:4096 --dir /tmp/worktree --session sid-123"
        );
    }

    #[test]
    fn test_attach_command_with_session_no_dir() {
        let command = opencode_attach_command(Some("sid-123"), None);
        assert_eq!(
            command,
            "opencode attach http://127.0.0.1:4096 --session sid-123"
        );
    }

    #[test]
    fn test_attach_command_without_session_or_dir_uses_attach_base() {
        let command = opencode_attach_command(None, None);
        assert_eq!(command, "opencode attach http://127.0.0.1:4096");
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

    struct EnvVarGuard {
        key: &'static str,
        previous: Option<String>,
    }

    impl EnvVarGuard {
        fn set(key: &'static str, value: impl AsRef<str>) -> Self {
            let previous = env::var(key).ok();
            unsafe {
                env::set_var(key, value.as_ref());
            }

            Self { key, previous }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            if let Some(previous) = &self.previous {
                unsafe {
                    env::set_var(self.key, previous);
                }
            } else {
                unsafe {
                    env::remove_var(self.key);
                }
            }
        }
    }

    fn spawn_session_lookup_server(
        response_body: &str,
    ) -> Result<(u16, thread::JoinHandle<String>)> {
        let listener = TcpListener::bind(("127.0.0.1", 0))?;
        let port = listener.local_addr()?.port();
        let body = response_body.to_string();

        let handle = thread::spawn(move || {
            let (mut stream, _) = listener
                .accept()
                .expect("mock session lookup server should accept request");

            let mut request = [0u8; 2048];
            let bytes_read = stream
                .read(&mut request)
                .expect("mock session lookup server should read request");
            let request_text = String::from_utf8_lossy(&request[..bytes_read]).to_string();

            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream
                .write_all(response.as_bytes())
                .expect("mock session lookup server should write response");

            request_text
        });

        Ok((port, handle))
    }

    #[test]
    fn test_query_session_by_dir_uses_api_with_directory_filter() -> Result<()> {
        let _guard = TEST_ENV_LOCK.lock().expect("test env mutex should lock");
        let working_dir = tempfile::tempdir()?;
        let (port, handle) =
            spawn_session_lookup_server(r#"[{"id":"sid-latest","directory":"/tmp/project"}]"#)?;
        let _port_guard = EnvVarGuard::set("OPENCODE_KANBAN_STATUS_PORT", port.to_string());

        let found = opencode_query_session_by_dir(working_dir.path())?;
        assert_eq!(found.as_deref(), Some("sid-latest"));

        let request_text = handle
            .join()
            .expect("mock session lookup server thread should join");
        let encoded_directory = encode(&working_dir.path().to_string_lossy()).to_string();
        assert!(
            request_text.starts_with(&format!("GET /session?directory={encoded_directory}")),
            "request did not include expected encoded directory filter: {request_text}"
        );

        Ok(())
    }

    #[test]
    fn test_query_session_by_dir_returns_none_when_no_sessions() -> Result<()> {
        let _guard = TEST_ENV_LOCK.lock().expect("test env mutex should lock");
        let working_dir = tempfile::tempdir()?;
        let (port, handle) = spawn_session_lookup_server("[]")?;
        let _port_guard = EnvVarGuard::set("OPENCODE_KANBAN_STATUS_PORT", port.to_string());

        let found = opencode_query_session_by_dir(working_dir.path())?;
        assert_eq!(found, None);

        let _ = handle
            .join()
            .expect("mock session lookup server thread should join");
        Ok(())
    }
}
