use std::env;
use std::path::Path;
use std::process::Command;
use std::sync::LazyLock;

use anyhow::{Context, Result, bail};
use regex::Regex;

use crate::tmux::{tmux_capture_pane, tmux_get_pane_pid};

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum Status {
    Running,
    Waiting,
    Idle,
    Dead,
    Unknown,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Running => "running",
            Status::Waiting => "waiting",
            Status::Idle => "idle",
            Status::Dead => "dead",
            Status::Unknown => "unknown",
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

    Status::Unknown
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

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::sync::{LazyLock, Mutex, MutexGuard};

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
}
