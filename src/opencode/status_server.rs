use std::collections::HashMap;
use std::time::{Duration, SystemTime};

use reqwest::Client;
use reqwest::StatusCode;
use serde_json::Value;
use urlencoding::encode;

use crate::types::{
    SessionState, SessionStatus, SessionStatusError, SessionStatusSource, SessionTodoItem,
};

#[derive(Debug, Clone)]
pub struct ServerStatusProvider {
    config: ServerStatusConfig,
    client: Client,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct SessionStatusMatch {
    pub session_id: String,
    pub parent_session_id: Option<String>,
    pub status: SessionStatus,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct SessionRecord {
    pub session_id: String,
    pub directory: String,
    pub parent_session_id: Option<String>,
}

impl SessionStatusMatch {
    pub fn is_root_session(&self) -> bool {
        self.parent_session_id.is_none()
    }
}

#[derive(Debug, Clone)]
pub struct ServerStatusConfig {
    pub hostname: String,
    pub port: u16,
    pub request_timeout: Duration,
}

impl Default for ServerStatusConfig {
    fn default() -> Self {
        let port = std::env::var("OPENCODE_KANBAN_STATUS_PORT")
            .ok()
            .and_then(|v| v.parse::<u16>().ok())
            .unwrap_or(4096);
        Self {
            hostname: "127.0.0.1".to_string(),
            port,
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

    fn session_todo_url(&self, session_id: &str) -> String {
        format!("{}/session/{}/todo", self.base_url(), encode(session_id))
    }

    pub async fn list_all_sessions(&self) -> Result<Vec<(String, String)>, SessionStatusError> {
        let records = self.list_all_session_records().await?;
        Ok(records
            .into_iter()
            .map(|record| (record.session_id, record.directory))
            .collect())
    }

    pub async fn list_all_session_records(&self) -> Result<Vec<SessionRecord>, SessionStatusError> {
        let response = self
            .client
            .get(self.session_url())
            .send()
            .await
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
            .await
            .map_err(|err| map_reqwest_error(err, "SERVER_READ_FAILED"))?;

        parse_session_records_body(&body)
    }

    pub async fn fetch_session_parent_map(
        &self,
    ) -> Result<HashMap<String, Option<String>>, SessionStatusError> {
        let records = self.list_all_session_records().await?;
        Ok(records
            .into_iter()
            .map(|record| (record.session_id, record.parent_session_id))
            .collect())
    }

    pub async fn fetch_all_statuses(
        &self,
        fetched_at: SystemTime,
        directory: Option<&str>,
    ) -> Result<HashMap<String, SessionStatus>, SessionStatusError> {
        let status_matches = self.fetch_status_matches(fetched_at, directory).await?;
        Ok(status_matches
            .into_iter()
            .map(|status_match| (status_match.session_id, status_match.status))
            .collect())
    }

    pub async fn fetch_status_matches(
        &self,
        fetched_at: SystemTime,
        directory: Option<&str>,
    ) -> Result<Vec<SessionStatusMatch>, SessionStatusError> {
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
            .await
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
            .await
            .map_err(|err| map_reqwest_error(err, "SERVER_READ_FAILED"))?;

        parse_status_matches_body(&body, fetched_at)
    }

    pub async fn fetch_session_todo(
        &self,
        session_id: &str,
    ) -> Result<Vec<SessionTodoItem>, SessionStatusError> {
        let response = self
            .client
            .get(self.session_todo_url(session_id))
            .send()
            .await
            .map_err(|err| map_reqwest_error(err, "SERVER_CONNECT_FAILED"))?;

        let status_code = response.status();
        if status_code == StatusCode::UNAUTHORIZED {
            return Err(SessionStatusError {
                code: "SERVER_AUTH_ERROR".to_string(),
                message: format!(
                    "OpenCode server rejected todo fetch for session {session_id} with HTTP 401"
                ),
            });
        }
        if status_code != StatusCode::OK {
            return Err(SessionStatusError {
                code: "SERVER_HTTP_ERROR".to_string(),
                message: format!(
                    "OpenCode server returned HTTP {status_code} for /session/{session_id}/todo"
                ),
            });
        }

        let body = response
            .text()
            .await
            .map_err(|err| map_reqwest_error(err, "SERVER_READ_FAILED"))?;

        parse_session_todo_body(&body)
    }
}

fn parse_status_matches_body(
    body: &str,
    fetched_at: SystemTime,
) -> Result<Vec<SessionStatusMatch>, SessionStatusError> {
    let payload: Value = serde_json::from_str(body).map_err(|err| SessionStatusError {
        code: "SERVER_CONTRACT_PARSE_ERROR".to_string(),
        message: format!("failed to parse /session/status response JSON: {err}"),
    })?;

    let object = payload.as_object().ok_or_else(|| SessionStatusError {
        code: "SERVER_CONTRACT_PARSE_ERROR".to_string(),
        message: "expected /session/status response to be a JSON object keyed by session id"
            .to_string(),
    })?;

    let mut statuses = Vec::with_capacity(object.len());
    for (session_id, value) in object {
        let state = parse_session_state(value).map_err(|err| SessionStatusError {
            code: "SERVER_CONTRACT_PARSE_ERROR".to_string(),
            message: format!("invalid /session/status entry for session {session_id}: {err}"),
        })?;
        let parent_session_id = parse_parent_session_id(value);

        statuses.push(SessionStatusMatch {
            session_id: session_id.clone(),
            parent_session_id,
            status: SessionStatus {
                state,
                source: SessionStatusSource::Server,
                fetched_at,
                error: None,
            },
        });
    }

    Ok(statuses)
}

fn parse_parent_session_id(value: &Value) -> Option<String> {
    const PARENT_SESSION_ID_KEYS: &[&str] = &[
        "parentSessionId",
        "parent_session_id",
        "parentSessionID",
        "parentId",
        "parent_id",
    ];

    let obj = value.as_object()?;
    for key in PARENT_SESSION_ID_KEYS {
        if let Some(parent_session_id) = obj.get(*key).and_then(Value::as_str) {
            let trimmed = parent_session_id.trim();
            if trimmed.is_empty() {
                return None;
            }
            return Some(trimmed.to_string());
        }
    }

    None
}

fn parse_session_records_body(body: &str) -> Result<Vec<SessionRecord>, SessionStatusError> {
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
        let parent_session_id = parse_parent_session_id(item);

        sessions.push(SessionRecord {
            session_id: session_id.to_string(),
            directory: directory.to_string(),
            parent_session_id,
        });
    }

    Ok(sessions)
}

fn parse_session_todo_body(body: &str) -> Result<Vec<SessionTodoItem>, SessionStatusError> {
    let payload: Value = serde_json::from_str(body).map_err(|err| SessionStatusError {
        code: "SERVER_CONTRACT_PARSE_ERROR".to_string(),
        message: format!("failed to parse /session/:id/todo response JSON: {err}"),
    })?;

    let array = payload.as_array().ok_or_else(|| SessionStatusError {
        code: "SERVER_CONTRACT_PARSE_ERROR".to_string(),
        message: "expected /session/:id/todo response to be a JSON array".to_string(),
    })?;

    let mut todos = Vec::with_capacity(array.len());
    for (index, todo) in array.iter().enumerate() {
        let parsed = parse_session_todo_item(todo).map_err(|err| SessionStatusError {
            code: "SERVER_CONTRACT_PARSE_ERROR".to_string(),
            message: format!("invalid /session/:id/todo entry at index {index}: {err}"),
        })?;
        todos.push(parsed);
    }

    Ok(todos)
}

fn parse_session_todo_item(value: &Value) -> Result<SessionTodoItem, &'static str> {
    if let Some(content) = value.as_str() {
        return Ok(SessionTodoItem {
            content: content.to_string(),
            completed: false,
        });
    }

    let obj = value
        .as_object()
        .ok_or("expected todo entry to be a string or object")?;

    let content = obj
        .get("content")
        .and_then(Value::as_str)
        .or_else(|| obj.get("text").and_then(Value::as_str))
        .or_else(|| obj.get("title").and_then(Value::as_str))
        .or_else(|| obj.get("label").and_then(Value::as_str))
        .ok_or("expected todo object to contain `content`, `text`, `title`, or `label` string")?;

    let completed = obj
        .get("completed")
        .and_then(Value::as_bool)
        .or_else(|| obj.get("done").and_then(Value::as_bool))
        .or_else(|| {
            obj.get("status")
                .and_then(Value::as_str)
                .map(is_completed_status)
        })
        .or_else(|| {
            obj.get("state")
                .and_then(Value::as_str)
                .map(is_completed_status)
        })
        .unwrap_or(false);

    Ok(SessionTodoItem {
        content: content.to_string(),
        completed,
    })
}

fn is_completed_status(raw: &str) -> bool {
    matches!(
        raw.trim().to_ascii_lowercase().as_str(),
        "done" | "completed" | "complete" | "closed"
    )
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
                "retry" => Ok(SessionState::Idle),
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
    match state.trim().to_ascii_lowercase().as_str() {
        "running" | "active" | "thinking" | "processing" => Ok(SessionState::Running),
        "waiting" | "blocked" | "prompt" | "paused" | "idle" | "ready" | "dead" | "stopped"
        | "offline" | "completed" | "unknown" => Ok(SessionState::Idle),
        _ => Err("unrecognized session state value"),
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
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    use super::*;

    #[tokio::test(flavor = "multi_thread")]
    async fn list_statuses_server_success_marks_server_source() {
        let port = spawn_single_response_server(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{\"sid-1\":{\"state\":\"running\"},\"sid-2\":\"idle\"}".to_string(),
        )
        .await;
        let provider = ServerStatusProvider::new(ServerStatusConfig {
            port,
            request_timeout: Duration::from_millis(500),
            ..ServerStatusConfig::default()
        });

        let results = provider
            .fetch_all_statuses(SystemTime::UNIX_EPOCH, None)
            .await
            .expect("status fetch should succeed");

        assert_eq!(results.len(), 2);
        assert_eq!(results["sid-1"].state, SessionState::Running);
        assert_eq!(results["sid-1"].source, SessionStatusSource::Server);
        assert!(results["sid-1"].error.is_none());
        assert_eq!(results["sid-2"].state, SessionState::Idle);
        assert_eq!(results["sid-2"].source, SessionStatusSource::Server);
        assert!(results["sid-2"].error.is_none());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn list_statuses_partial_response_marks_missing_entries() {
        let port = spawn_single_response_server(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{\"sid-1\":{\"state\":\"running\"}}".to_string(),
        )
        .await;
        let provider = ServerStatusProvider::new(ServerStatusConfig {
            port,
            request_timeout: Duration::from_millis(500),
            ..ServerStatusConfig::default()
        });

        let results = provider
            .fetch_all_statuses(SystemTime::UNIX_EPOCH, None)
            .await
            .expect("status fetch should succeed");

        assert_eq!(results["sid-1"].source, SessionStatusSource::Server);
        assert!(results["sid-1"].error.is_none());
        assert!(!results.contains_key("sid-2"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn list_statuses_timeout_sets_timeout_error() {
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("listener should bind");
        let port = listener
            .local_addr()
            .expect("listener should have local addr")
            .port();

        tokio::spawn(async move {
            let (mut stream, _) = listener
                .accept()
                .await
                .expect("server should accept one socket");
            let mut request = [0u8; 512];
            let _ = stream.read(&mut request).await;
            tokio::time::sleep(Duration::from_millis(200)).await;
            let _ = stream.write_all(b"").await;
        });

        let provider = ServerStatusProvider::new(ServerStatusConfig {
            port,
            request_timeout: Duration::from_millis(50),
            ..ServerStatusConfig::default()
        });

        let err = provider
            .fetch_all_statuses(SystemTime::UNIX_EPOCH, None)
            .await
            .expect_err("status fetch should time out");
        assert_eq!(Some(err.code.as_str()), Some("SERVER_TIMEOUT"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn list_statuses_auth_error_sets_auth_code() {
        let port = spawn_single_response_server(
            "HTTP/1.1 401 Unauthorized\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{\"error\":\"unauthorized\"}".to_string(),
        )
        .await;
        let provider = ServerStatusProvider::new(ServerStatusConfig {
            port,
            request_timeout: Duration::from_millis(500),
            ..ServerStatusConfig::default()
        });

        let err = provider
            .fetch_all_statuses(SystemTime::UNIX_EPOCH, None)
            .await
            .expect_err("status fetch should fail with auth");
        assert_eq!(Some(err.code.as_str()), Some("SERVER_AUTH_ERROR"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn fetch_status_matches_parses_parent_session_id() {
        let port = spawn_single_response_server(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{\"root\":{\"state\":\"running\"},\"sub\":{\"state\":\"idle\",\"parentSessionId\":\"root\"}}".to_string(),
        )
        .await;
        let provider = ServerStatusProvider::new(ServerStatusConfig {
            port,
            request_timeout: Duration::from_millis(500),
            ..ServerStatusConfig::default()
        });

        let matches = provider
            .fetch_status_matches(SystemTime::UNIX_EPOCH, None)
            .await
            .expect("status matches should parse");

        let root = matches
            .iter()
            .find(|status_match| status_match.session_id == "root")
            .expect("root match should exist");
        assert!(root.parent_session_id.is_none());

        let sub = matches
            .iter()
            .find(|status_match| status_match.session_id == "sub")
            .expect("sub match should exist");
        assert_eq!(sub.parent_session_id.as_deref(), Some("root"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn list_all_session_records_parses_parent_session_id() {
        let port = spawn_single_response_server(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n[{\"id\":\"root\",\"directory\":\"/repo\"},{\"id\":\"sub\",\"directory\":\"/repo\",\"parentSessionId\":\"root\"}]".to_string(),
        )
        .await;
        let provider = ServerStatusProvider::new(ServerStatusConfig {
            port,
            request_timeout: Duration::from_millis(500),
            ..ServerStatusConfig::default()
        });

        let records = provider
            .list_all_session_records()
            .await
            .expect("session records should parse");

        let root = records
            .iter()
            .find(|record| record.session_id == "root")
            .expect("root record should exist");
        assert!(root.parent_session_id.is_none());

        let sub = records
            .iter()
            .find(|record| record.session_id == "sub")
            .expect("sub record should exist");
        assert_eq!(sub.parent_session_id.as_deref(), Some("root"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn fetch_session_todo_parses_todo_array() {
        let port = spawn_single_response_server(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n[{\"content\":\"Write tests\",\"completed\":false},{\"title\":\"Ship release\",\"status\":\"done\"}]".to_string(),
        )
        .await;
        let provider = ServerStatusProvider::new(ServerStatusConfig {
            port,
            request_timeout: Duration::from_millis(500),
            ..ServerStatusConfig::default()
        });

        let todos = provider
            .fetch_session_todo("sid-1")
            .await
            .expect("todo response should parse");

        assert_eq!(todos.len(), 2);
        assert_eq!(todos[0].content, "Write tests");
        assert!(!todos[0].completed);
        assert_eq!(todos[1].content, "Ship release");
        assert!(todos[1].completed);
    }

    async fn spawn_single_response_server(response: String) -> u16 {
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("listener should bind");
        let port = listener
            .local_addr()
            .expect("listener should have local addr")
            .port();

        tokio::spawn(async move {
            let (mut stream, _) = listener
                .accept()
                .await
                .expect("server should accept one socket");
            let mut request = [0u8; 512];
            match stream.read(&mut request).await {
                Ok(_) => {}
                Err(err) if err.kind() == std::io::ErrorKind::ConnectionReset => {}
                Err(err) => panic!("server should read request: {err}"),
            }
            stream
                .write_all(response.as_bytes())
                .await
                .expect("server should write response");
        });

        port
    }
}
