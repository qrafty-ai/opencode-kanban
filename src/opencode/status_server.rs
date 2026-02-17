use std::collections::HashMap;
use std::time::{Duration, SystemTime};

use reqwest::StatusCode;
use reqwest::blocking::Client;
use serde_json::Value;
use urlencoding::encode;

use crate::types::{SessionState, SessionStatus, SessionStatusError, SessionStatusSource};

use super::StatusProvider;

#[derive(Debug, Clone)]
pub struct ServerStatusProvider {
    config: ServerStatusConfig,
    client: Client,
}

#[derive(Debug, Clone)]
pub struct ServerStatusConfig {
    pub hostname: String,
    pub port: u16,
    pub request_timeout: Duration,
}

impl Default for ServerStatusConfig {
    fn default() -> Self {
        Self {
            hostname: "127.0.0.1".to_string(),
            port: 4096,
            request_timeout: Duration::from_millis(300),
        }
    }
}

impl Default for ServerStatusProvider {
    fn default() -> Self {
        Self::new(ServerStatusConfig::default())
    }
}

impl ServerStatusProvider {
    pub fn new(config: ServerStatusConfig) -> Self {
        let client = Client::builder()
            .timeout(config.request_timeout)
            .build()
            .expect("failed to build status client");

        Self { config, client }
    }

    fn base_url(&self) -> String {
        format!("http://{}:{}", self.config.hostname, self.config.port)
    }

    fn session_url(&self) -> String {
        format!("{}/session", self.base_url())
    }

    fn session_status_url(&self) -> String {
        format!("{}/session/status", self.base_url())
    }

    pub fn list_all_sessions(&self) -> Result<Vec<(String, String)>, SessionStatusError> {
        let response = self
            .client
            .get(self.session_url())
            .send()
            .map_err(|err| map_reqwest_error(err, "SERVER_CONNECT_FAILED"))?;

        let status_code = response.status();
        if status_code == StatusCode::UNAUTHORIZED {
            return Err(SessionStatusError {
                code: "SERVER_AUTH_ERROR".to_string(),
                message: "OpenCode server rejected session list with HTTP 401".to_string(),
            });
        }
        if status_code != StatusCode::OK {
            return Err(SessionStatusError {
                code: "SERVER_HTTP_ERROR".to_string(),
                message: format!("OpenCode server returned HTTP {status_code} for /session"),
            });
        }

        let body = response
            .text()
            .map_err(|err| map_reqwest_error(err, "SERVER_READ_FAILED"))?;

        parse_sessions_body(&body)
    }

    pub fn fetch_all_statuses(
        &self,
        fetched_at: SystemTime,
        directory: Option<&str>,
    ) -> Result<HashMap<String, SessionStatus>, SessionStatusError> {
        let status_url = if let Some(directory) = directory {
            format!(
                "{}?directory={}",
                self.session_status_url(),
                encode(directory)
            )
        } else {
            self.session_status_url()
        };

        let response = self
            .client
            .get(status_url)
            .send()
            .map_err(|err| map_reqwest_error(err, "SERVER_CONNECT_FAILED"))?;

        let status_code = response.status();
        if status_code == StatusCode::UNAUTHORIZED {
            return Err(SessionStatusError {
                code: "SERVER_AUTH_ERROR".to_string(),
                message: "OpenCode server rejected status poll with HTTP 401".to_string(),
            });
        }
        if status_code != StatusCode::OK {
            return Err(SessionStatusError {
                code: "SERVER_HTTP_ERROR".to_string(),
                message: format!("OpenCode server returned HTTP {status_code} for /session/status"),
            });
        }

        let body = response
            .text()
            .map_err(|err| map_reqwest_error(err, "SERVER_READ_FAILED"))?;

        parse_status_body(&body, fetched_at)
    }
}

impl StatusProvider for ServerStatusProvider {
    fn get_status(&self, session_id: &str) -> SessionStatus {
        self.list_statuses(&[session_id.to_string()])
            .into_iter()
            .next()
            .map(|(_, status)| status)
            .unwrap_or_else(|| status_with_error(SystemTime::now(), missing_error(session_id)))
    }

    fn list_statuses(&self, session_ids: &[String]) -> Vec<(String, SessionStatus)> {
        if session_ids.is_empty() {
            return Vec::new();
        }

        let fetched_at = SystemTime::now();
        match self.fetch_all_statuses(fetched_at, None) {
            Ok(status_map) => session_ids
                .iter()
                .map(|session_id| {
                    let status = status_map.get(session_id).cloned().unwrap_or_else(|| {
                        status_with_error(fetched_at, missing_error(session_id))
                    });
                    (session_id.clone(), status)
                })
                .collect(),
            Err(err) => session_ids
                .iter()
                .map(|session_id| {
                    (
                        session_id.clone(),
                        status_with_error(fetched_at, err.clone()),
                    )
                })
                .collect(),
        }
    }
}

fn parse_status_body(
    body: &str,
    fetched_at: SystemTime,
) -> Result<HashMap<String, SessionStatus>, SessionStatusError> {
    let payload: Value = serde_json::from_str(body).map_err(|err| SessionStatusError {
        code: "SERVER_CONTRACT_PARSE_ERROR".to_string(),
        message: format!("failed to parse /session/status response JSON: {err}"),
    })?;

    let object = payload.as_object().ok_or_else(|| SessionStatusError {
        code: "SERVER_CONTRACT_PARSE_ERROR".to_string(),
        message: "expected /session/status response to be a JSON object keyed by session id"
            .to_string(),
    })?;

    let mut statuses = HashMap::new();
    for (session_id, value) in object {
        let state = parse_session_state(value).map_err(|err| SessionStatusError {
            code: "SERVER_CONTRACT_PARSE_ERROR".to_string(),
            message: format!("invalid /session/status entry for session {session_id}: {err}"),
        })?;

        statuses.insert(
            session_id.clone(),
            SessionStatus {
                state,
                source: SessionStatusSource::Server,
                fetched_at,
                error: None,
            },
        );
    }

    Ok(statuses)
}

fn parse_sessions_body(body: &str) -> Result<Vec<(String, String)>, SessionStatusError> {
    let payload: Value = serde_json::from_str(body).map_err(|err| SessionStatusError {
        code: "SERVER_CONTRACT_PARSE_ERROR".to_string(),
        message: format!("failed to parse /session response JSON: {err}"),
    })?;

    let array = payload.as_array().ok_or_else(|| SessionStatusError {
        code: "SERVER_CONTRACT_PARSE_ERROR".to_string(),
        message: "expected /session response to be a JSON array".to_string(),
    })?;

    let mut sessions = Vec::new();
    for item in array {
        let obj = item.as_object().ok_or_else(|| SessionStatusError {
            code: "SERVER_CONTRACT_PARSE_ERROR".to_string(),
            message: "expected /session array entries to be objects".to_string(),
        })?;

        let Some(session_id) = obj.get("id").and_then(|v| v.as_str()) else {
            continue;
        };
        let directory = obj.get("directory").and_then(|v| v.as_str()).unwrap_or("");

        sessions.push((session_id.to_string(), directory.to_string()));
    }

    Ok(sessions)
}

fn parse_session_state(value: &Value) -> Result<SessionState, &'static str> {
    if let Some(raw) = value.as_str() {
        return parse_state_str(raw);
    }

    if let Some(obj) = value.as_object() {
        // OpenCode format: { "type": "idle" | "busy" | "retry" }
        if let Some(typ) = obj.get("type").and_then(Value::as_str) {
            return match typ {
                "idle" => Ok(SessionState::Idle),
                "busy" => Ok(SessionState::Running),
                "retry" => Ok(SessionState::Waiting),
                _ => Err("unrecognized session type value"),
            };
        }
        // Legacy format: { "state": "running" } or { "status": "running" }
        if let Some(raw) = obj.get("state").and_then(Value::as_str) {
            return parse_state_str(raw);
        }
        if let Some(raw) = obj.get("status").and_then(Value::as_str) {
            return parse_state_str(raw);
        }
        return Err("expected object entry to contain `type`, `state`, or `status` string");
    }

    Err("expected status entry to be a string or object")
}

fn parse_state_str(state: &str) -> Result<SessionState, &'static str> {
    let normalized = state.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "running" | "active" | "thinking" | "processing" => Ok(SessionState::Running),
        "waiting" | "blocked" | "prompt" | "paused" => Ok(SessionState::Waiting),
        "idle" | "ready" => Ok(SessionState::Idle),
        "dead" | "stopped" | "offline" | "completed" => Ok(SessionState::Dead),
        "unknown" => Ok(SessionState::Idle),
        _ => Err("unrecognized session state value"),
    }
}

fn status_with_error(fetched_at: SystemTime, error: SessionStatusError) -> SessionStatus {
    SessionStatus {
        state: SessionState::Idle,
        source: SessionStatusSource::None,
        fetched_at,
        error: Some(error),
    }
}

fn missing_error(session_id: &str) -> SessionStatusError {
    SessionStatusError {
        code: "SERVER_STATUS_MISSING".to_string(),
        message: format!("/session/status did not include session id {session_id}"),
    }
}

fn map_reqwest_error(err: reqwest::Error, default_code: &str) -> SessionStatusError {
    let code = if err.is_timeout() {
        "SERVER_TIMEOUT"
    } else {
        default_code
    };

    SessionStatusError {
        code: code.to_string(),
        message: err.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use std::io::{self, Read, Write};
    use std::net::TcpListener;
    use std::thread;

    use super::*;

    #[test]
    fn list_statuses_server_success_marks_server_source() {
        let port = spawn_single_response_server(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{\"sid-1\":{\"state\":\"running\"},\"sid-2\":\"idle\"}".to_string(),
        );
        let provider = ServerStatusProvider::new(ServerStatusConfig {
            port,
            request_timeout: Duration::from_millis(500),
            ..ServerStatusConfig::default()
        });

        let results = provider.list_statuses(&["sid-1".to_string(), "sid-2".to_string()]);

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].1.state, SessionState::Running);
        assert_eq!(results[0].1.source, SessionStatusSource::Server);
        assert!(results[0].1.error.is_none());
        assert_eq!(results[1].1.state, SessionState::Idle);
        assert_eq!(results[1].1.source, SessionStatusSource::Server);
        assert!(results[1].1.error.is_none());
    }

    #[test]
    fn list_statuses_partial_response_marks_missing_entries() {
        let port = spawn_single_response_server(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{\"sid-1\":{\"state\":\"running\"}}".to_string(),
        );
        let provider = ServerStatusProvider::new(ServerStatusConfig {
            port,
            request_timeout: Duration::from_millis(500),
            ..ServerStatusConfig::default()
        });

        let results = provider.list_statuses(&["sid-1".to_string(), "sid-2".to_string()]);

        assert_eq!(results[0].1.source, SessionStatusSource::Server);
        assert!(results[0].1.error.is_none());

        let missing = &results[1].1;
        assert_eq!(missing.source, SessionStatusSource::None);
        assert_eq!(missing.state, SessionState::Idle);
        assert_eq!(
            missing.error.as_ref().map(|err| err.code.as_str()),
            Some("SERVER_STATUS_MISSING")
        );
    }

    #[test]
    fn list_statuses_timeout_sets_timeout_error() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("listener should bind");
        let port = listener
            .local_addr()
            .expect("listener should have local addr")
            .port();

        thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("server should accept one socket");
            let mut request = [0u8; 512];
            let _ = stream.read(&mut request);
            thread::sleep(Duration::from_millis(200));
            let _ = stream.write_all(b"");
        });

        let provider = ServerStatusProvider::new(ServerStatusConfig {
            port,
            request_timeout: Duration::from_millis(50),
            ..ServerStatusConfig::default()
        });

        let results = provider.list_statuses(&["sid-1".to_string()]);

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].1.source, SessionStatusSource::None);
        assert_eq!(
            results[0].1.error.as_ref().map(|err| err.code.as_str()),
            Some("SERVER_TIMEOUT")
        );
    }

    #[test]
    fn list_statuses_auth_error_sets_auth_code() {
        let port = spawn_single_response_server(
            "HTTP/1.1 401 Unauthorized\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{\"error\":\"unauthorized\"}".to_string(),
        );
        let provider = ServerStatusProvider::new(ServerStatusConfig {
            port,
            request_timeout: Duration::from_millis(500),
            ..ServerStatusConfig::default()
        });

        let results = provider.list_statuses(&["sid-1".to_string()]);

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].1.source, SessionStatusSource::None);
        assert_eq!(
            results[0].1.error.as_ref().map(|err| err.code.as_str()),
            Some("SERVER_AUTH_ERROR")
        );
    }

    fn spawn_single_response_server(response: String) -> u16 {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("listener should bind");
        let port = listener
            .local_addr()
            .expect("listener should have local addr")
            .port();

        thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("server should accept one socket");
            let mut request = [0u8; 512];
            match stream.read(&mut request) {
                Ok(_) => {}
                Err(err) if err.kind() == io::ErrorKind::ConnectionReset => {}
                Err(err) => panic!("server should read request: {err}"),
            }
            stream
                .write_all(response.as_bytes())
                .expect("server should write response");
        });

        port
    }
}
