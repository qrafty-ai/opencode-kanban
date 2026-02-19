use reqwest::StatusCode;
use reqwest::blocking::Client;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Debug, Clone, Default, Eq, PartialEq)]
pub enum OpenCodeServerState {
    #[default]
    Starting,
    ReadyAttached,
    ReadySpawned,
    Failed(String),
}

#[derive(Debug, Clone, Default)]
pub struct OpenCodeServerManager {
    state: Arc<Mutex<OpenCodeServerState>>,
}

impl OpenCodeServerManager {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(OpenCodeServerState::Starting)),
        }
    }

    pub fn status(&self) -> OpenCodeServerState {
        match self.state.lock() {
            Ok(state) => state.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }

    pub fn is_ready(&self) -> bool {
        matches!(
            self.status(),
            OpenCodeServerState::ReadyAttached | OpenCodeServerState::ReadySpawned
        )
    }
}

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub hostname: String,
    pub port: u16,
    pub request_timeout: Duration,
    pub startup_timeout: Duration,
    pub initial_backoff: Duration,
    pub max_backoff: Duration,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            hostname: "127.0.0.1".to_string(),
            port: 4096,
            request_timeout: Duration::from_millis(300),
            startup_timeout: Duration::from_secs(5),
            initial_backoff: Duration::from_millis(100),
            max_backoff: Duration::from_millis(800),
        }
    }
}

pub fn ensure_server_ready() -> OpenCodeServerManager {
    ensure_server_ready_with_config(ServerConfig::default())
}

fn ensure_server_ready_with_config(config: ServerConfig) -> OpenCodeServerManager {
    let manager = OpenCodeServerManager::new();
    let state = Arc::clone(&manager.state);

    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        handle.spawn(async move {
            let next_state = match tokio::task::spawn_blocking(move || {
                let runtime = RealServerRuntime;
                bootstrap_server(&runtime, &config)
            })
            .await
            {
                Ok(state) => state,
                Err(err) => OpenCodeServerState::Failed(format!(
                    "failed to bootstrap OpenCode server task: {err}"
                )),
            };

            match state.lock() {
                Ok(mut current) => *current = next_state,
                Err(poisoned) => {
                    *poisoned.into_inner() = next_state;
                }
            }
        });
    } else {
        let runtime = RealServerRuntime;
        let next_state = bootstrap_server(&runtime, &config);

        match state.lock() {
            Ok(mut current) => *current = next_state,
            Err(poisoned) => {
                *poisoned.into_inner() = next_state;
            }
        }
    }

    manager
}

trait ServerRuntime {
    fn check_health(&self, config: &ServerConfig) -> bool;
    fn spawn_server(&self, binary: &str, config: &ServerConfig) -> Result<(), String>;
    fn sleep(&self, duration: Duration);
}

struct RealServerRuntime;

impl ServerRuntime for RealServerRuntime {
    fn check_health(&self, config: &ServerConfig) -> bool {
        check_server_health(config)
    }

    fn spawn_server(&self, binary: &str, config: &ServerConfig) -> Result<(), String> {
        spawn_opencode_server(binary, config)
    }

    fn sleep(&self, duration: Duration) {
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.block_on(tokio::time::sleep(duration));
            return;
        }

        if let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
        {
            runtime.block_on(tokio::time::sleep(duration));
        }
    }
}

fn bootstrap_server(runtime: &impl ServerRuntime, config: &ServerConfig) -> OpenCodeServerState {
    if runtime.check_health(config) {
        return OpenCodeServerState::ReadyAttached;
    }

    let binary = super::opencode_binary();
    if let Err(err) = runtime.spawn_server(&binary, config) {
        return OpenCodeServerState::Failed(err);
    }

    let mut remaining = config.startup_timeout;
    let mut backoff = config.initial_backoff;

    while remaining > Duration::ZERO {
        if runtime.check_health(config) {
            return OpenCodeServerState::ReadySpawned;
        }

        let wait_for = remaining.min(backoff);
        runtime.sleep(wait_for);
        remaining = remaining.saturating_sub(wait_for);
        backoff = backoff.saturating_mul(2).min(config.max_backoff);
    }

    if runtime.check_health(config) {
        return OpenCodeServerState::ReadySpawned;
    }

    OpenCodeServerState::Failed(format!(
        "timed out waiting for OpenCode server at {}:{} after {}ms",
        config.hostname,
        config.port,
        config.startup_timeout.as_millis()
    ))
}

fn check_server_health(config: &ServerConfig) -> bool {
    let client = match Client::builder().timeout(config.request_timeout).build() {
        Ok(client) => client,
        Err(_) => return false,
    };

    let url = format!("http://{}:{}/global/health", config.hostname, config.port);
    let response = match client.get(url).send() {
        Ok(response) => response,
        Err(_) => return false,
    };

    if response.status() != StatusCode::OK {
        return false;
    }

    match response.text() {
        Ok(body) => is_healthy_response(&body),
        Err(_) => false,
    }
}

fn is_healthy_response(response: &str) -> bool {
    response.contains("\"healthy\":true") || response.contains("\"healthy\": true")
}

fn spawn_opencode_server(binary: &str, config: &ServerConfig) -> Result<(), String> {
    let port = config.port.to_string();

    let mut cmd = Command::new(binary);
    if let Some(home_dir) = dirs::home_dir() {
        cmd.current_dir(home_dir);
    }

    cmd.args([
        "serve",
        "--port",
        port.as_str(),
        "--hostname",
        config.hostname.as_str(),
    ])
    .stdout(Stdio::null())
    .stderr(Stdio::null())
    .spawn()
    .map(|_| ())
    .map_err(|err| {
        format!(
            "failed to launch `{binary} serve --port {} --hostname {}`: {err}",
            config.port, config.hostname
        )
    })
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use super::*;

    struct FakeServerRuntime {
        health_checks: Mutex<VecDeque<bool>>,
        spawn_error: Option<String>,
        spawn_calls: Mutex<usize>,
        sleeps: Mutex<Vec<Duration>>,
    }

    impl FakeServerRuntime {
        fn new(health_checks: Vec<bool>, spawn_error: Option<String>) -> Self {
            Self {
                health_checks: Mutex::new(health_checks.into()),
                spawn_error,
                spawn_calls: Mutex::new(0),
                sleeps: Mutex::new(Vec::new()),
            }
        }

        fn spawn_calls(&self) -> usize {
            *self
                .spawn_calls
                .lock()
                .expect("spawn calls mutex should not be poisoned")
        }

        fn total_sleep(&self) -> Duration {
            self.sleeps
                .lock()
                .expect("sleep mutex should not be poisoned")
                .iter()
                .copied()
                .sum()
        }
    }

    impl ServerRuntime for FakeServerRuntime {
        fn check_health(&self, _config: &ServerConfig) -> bool {
            self.health_checks
                .lock()
                .expect("health checks mutex should not be poisoned")
                .pop_front()
                .unwrap_or(false)
        }

        fn spawn_server(&self, _binary: &str, _config: &ServerConfig) -> Result<(), String> {
            let mut calls = self
                .spawn_calls
                .lock()
                .expect("spawn calls mutex should not be poisoned");
            *calls += 1;

            match &self.spawn_error {
                Some(err) => Err(err.clone()),
                None => Ok(()),
            }
        }

        fn sleep(&self, duration: Duration) {
            self.sleeps
                .lock()
                .expect("sleep mutex should not be poisoned")
                .push(duration);
        }
    }

    fn test_config() -> ServerConfig {
        ServerConfig {
            request_timeout: Duration::from_millis(10),
            startup_timeout: Duration::from_millis(350),
            initial_backoff: Duration::from_millis(100),
            max_backoff: Duration::from_millis(200),
            ..ServerConfig::default()
        }
    }

    #[test]
    fn attach_success_skips_spawn() {
        let runtime = FakeServerRuntime::new(vec![true], None);

        let state = bootstrap_server(&runtime, &test_config());

        assert_eq!(state, OpenCodeServerState::ReadyAttached);
        assert_eq!(runtime.spawn_calls(), 0);
    }

    #[test]
    fn spawn_fallback_runs_once_and_becomes_ready() {
        let runtime = FakeServerRuntime::new(vec![false, false, true], None);

        let state = bootstrap_server(&runtime, &test_config());

        assert_eq!(state, OpenCodeServerState::ReadySpawned);
        assert_eq!(runtime.spawn_calls(), 1);
    }

    #[test]
    fn timeout_after_single_spawn_attempt() {
        let runtime = FakeServerRuntime::new(vec![false, false, false, false, false], None);

        let state = bootstrap_server(&runtime, &test_config());

        assert_eq!(runtime.spawn_calls(), 1);
        assert_eq!(runtime.total_sleep(), Duration::from_millis(350));
        assert!(
            matches!(state, OpenCodeServerState::Failed(message) if message.contains("timed out"))
        );
    }
}
