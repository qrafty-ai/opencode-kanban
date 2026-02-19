use std::collections::HashMap;
use std::time::{Duration, SystemTime};

use reqwest::Client;
use reqwest::StatusCode;
use serde_json::Value;
use urlencoding::encode;

use crate::types::{
    SessionMessageItem, SessionState, SessionStatus, SessionStatusError, SessionStatusSource,
    SessionTodoItem,
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
    pub title: Option<String>,
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

    fn session_message_url(&self, session_id: &str) -> String {
        format!("{}/session/{}/message", self.base_url(), encode(session_id))
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

    pub async fn fetch_session_messages(
        &self,
        session_id: &str,
    ) -> Result<Vec<SessionMessageItem>, SessionStatusError> {
        let response = self
            .client
            .get(self.session_message_url(session_id))
            .send()
            .await
            .map_err(|err| map_reqwest_error(err, "SERVER_CONNECT_FAILED"))?;

        let status_code = response.status();
        if status_code == StatusCode::UNAUTHORIZED {
            return Err(SessionStatusError {
                code: "SERVER_AUTH_ERROR".to_string(),
                message: format!(
                    "OpenCode server rejected message fetch for session {session_id} with HTTP 401"
                ),
            });
        }
        if status_code != StatusCode::OK {
            return Err(SessionStatusError {
                code: "SERVER_HTTP_ERROR".to_string(),
                message: format!(
                    "OpenCode server returned HTTP {status_code} for /session/{session_id}/message"
                ),
            });
        }

        let body = response
            .text()
            .await
            .map_err(|err| map_reqwest_error(err, "SERVER_READ_FAILED"))?;

        parse_session_message_body(&body)
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
        "parentID",
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
        let title = obj
            .get("title")
            .and_then(|v| v.as_str())
            .or_else(|| obj.get("name").and_then(|v| v.as_str()))
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        let parent_session_id = parse_parent_session_id(item);

        sessions.push(SessionRecord {
            session_id: session_id.to_string(),
            directory: directory.to_string(),
            title,
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

fn parse_session_message_body(body: &str) -> Result<Vec<SessionMessageItem>, SessionStatusError> {
    let payload: Value = serde_json::from_str(body).map_err(|err| SessionStatusError {
        code: "SERVER_CONTRACT_PARSE_ERROR".to_string(),
        message: format!("failed to parse /session/:id/message response JSON: {err}"),
    })?;

    let array = if let Some(array) = payload.as_array() {
        array
    } else if let Some(obj) = payload.as_object() {
        if let Some(array) = obj.get("messages").and_then(Value::as_array) {
            array
        } else if let Some(array) = obj.get("items").and_then(Value::as_array) {
            array
        } else if let Some(array) = obj.get("data").and_then(Value::as_array) {
            array
        } else {
            return Err(SessionStatusError {
                code: "SERVER_CONTRACT_PARSE_ERROR".to_string(),
                message: "expected /session/:id/message response to be a JSON array or object containing messages/items/data array".to_string(),
            });
        }
    } else {
        return Err(SessionStatusError {
            code: "SERVER_CONTRACT_PARSE_ERROR".to_string(),
            message: "expected /session/:id/message response to be a JSON array or object"
                .to_string(),
        });
    };

    let mut messages = Vec::with_capacity(array.len());
    for value in array {
        if let Ok(Some(parsed)) = parse_session_message_item(value) {
            messages.push(parsed);
        }
    }

    Ok(messages)
}

fn parse_session_message_item(value: &Value) -> Result<Option<SessionMessageItem>, &'static str> {
    if let Some(content) = value.as_str() {
        let content = content.trim();
        if content.is_empty() {
            return Ok(None);
        }
        return Ok(Some(SessionMessageItem {
            message_type: Some("text".to_string()),
            role: None,
            content: content.to_string(),
            timestamp: None,
        }));
    }

    let obj = value
        .as_object()
        .ok_or("expected message entry to be a string or object")?;

    let role = obj
        .get("role")
        .and_then(Value::as_str)
        .or_else(|| obj.get("type").and_then(Value::as_str))
        .or_else(|| obj.get("author").and_then(Value::as_str))
        .or_else(|| {
            obj.get("info")
                .and_then(Value::as_object)
                .and_then(|info| info.get("role"))
                .and_then(Value::as_str)
        })
        .or_else(|| {
            obj.get("author")
                .and_then(Value::as_object)
                .and_then(|author| author.get("role"))
                .and_then(Value::as_str)
        })
        .map(str::trim)
        .filter(|role| !role.is_empty())
        .map(str::to_string);

    let timestamp = extract_message_timestamp(value);

    let message_type = extract_message_type(value);

    let Some(content) = extract_message_content(value) else {
        return Ok(None);
    };
    if content.trim().is_empty() {
        return Ok(None);
    }

    Ok(Some(SessionMessageItem {
        message_type,
        role,
        content,
        timestamp,
    }))
}

fn extract_message_type(value: &Value) -> Option<String> {
    let obj = value.as_object()?;

    if let Some(kind) = obj
        .get("type")
        .and_then(Value::as_str)
        .and_then(normalize_message_type)
    {
        return Some(kind);
    }

    if let Some(kind) = obj
        .get("info")
        .and_then(Value::as_object)
        .and_then(|info| info.get("type"))
        .and_then(Value::as_str)
        .and_then(normalize_message_type)
    {
        return Some(kind);
    }

    if let Some(parts) = obj.get("parts").and_then(Value::as_array)
        && let Some(kind) = parts.iter().find_map(|part| {
            part.as_object()
                .and_then(|part_obj| part_obj.get("type"))
                .and_then(Value::as_str)
                .and_then(normalize_message_type)
        })
    {
        return Some(kind);
    }

    if let Some(data) = obj.get("data").and_then(Value::as_array)
        && let Some(kind) = data.iter().find_map(|entry| {
            entry
                .as_object()
                .and_then(|entry_obj| entry_obj.get("type"))
                .and_then(Value::as_str)
                .and_then(normalize_message_type)
        })
    {
        return Some(kind);
    }

    Some("text".to_string())
}

fn normalize_message_type(raw: &str) -> Option<String> {
    let normalized = raw.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return None;
    }

    match normalized.as_str() {
        "assistant" | "user" | "system" => Some("text".to_string()),
        _ => Some(normalized),
    }
}

fn extract_message_content(value: &Value) -> Option<String> {
    if let Some(array) = value.as_array() {
        let combined = array
            .iter()
            .filter_map(extract_message_content)
            .collect::<Vec<_>>()
            .join("\n");
        if !combined.trim().is_empty() {
            return Some(combined);
        }
        return None;
    }

    if let Some(raw) = value.as_str() {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return None;
        }
        return Some(trimmed.to_string());
    }

    let obj = value.as_object()?;

    const CONTENT_KEYS: &[&str] = &["content", "text", "message", "body", "value"];
    for key in CONTENT_KEYS {
        if let Some(content_value) = obj.get(*key)
            && let Some(text) = extract_message_content(content_value)
        {
            return Some(text);
        }
    }

    if let Some(parts) = obj.get("parts").and_then(Value::as_array) {
        let combined = parts
            .iter()
            .filter_map(extract_message_content)
            .collect::<Vec<_>>()
            .join("\n");
        if !combined.trim().is_empty() {
            return Some(combined);
        }
    }

    if let Some(data) = obj.get("data").and_then(Value::as_array) {
        let combined = data
            .iter()
            .filter_map(extract_message_content)
            .collect::<Vec<_>>()
            .join("\n");
        if !combined.trim().is_empty() {
            return Some(combined);
        }
    }

    None
}

fn extract_message_timestamp(value: &Value) -> Option<String> {
    let obj = value.as_object()?;

    if let Some(ts) = find_timestamp_in_object(obj) {
        return Some(ts);
    }

    if let Some(info) = obj.get("info").and_then(Value::as_object)
        && let Some(ts) = find_timestamp_in_object(info)
    {
        return Some(ts);
    }

    if let Some(parts) = obj.get("parts").and_then(Value::as_array)
        && let Some(ts) = parts.iter().find_map(extract_timestamp_from_value)
    {
        return Some(ts);
    }

    if let Some(data) = obj.get("data").and_then(Value::as_array)
        && let Some(ts) = data.iter().find_map(extract_timestamp_from_value)
    {
        return Some(ts);
    }

    None
}

fn find_timestamp_in_object(obj: &serde_json::Map<String, Value>) -> Option<String> {
    const DIRECT_KEYS: &[&str] = &["timestamp", "createdAt", "created_at"];
    for key in DIRECT_KEYS {
        if let Some(raw) = obj.get(*key)
            && let Some(parsed) = scalar_timestamp(raw)
        {
            return Some(parsed);
        }
    }

    if let Some(raw_time) = obj.get("time")
        && let Some(parsed) = extract_timestamp_from_value(raw_time)
    {
        return Some(parsed);
    }

    const NESTED_KEYS: &[&str] = &["created", "completed", "start", "end"];
    for key in NESTED_KEYS {
        if let Some(raw) = obj.get(*key)
            && let Some(parsed) = scalar_timestamp(raw)
        {
            return Some(parsed);
        }
    }

    None
}

fn extract_timestamp_from_value(value: &Value) -> Option<String> {
    if let Some(parsed) = scalar_timestamp(value) {
        return Some(parsed);
    }

    let obj = value.as_object()?;
    find_timestamp_in_object(obj)
}

fn scalar_timestamp(value: &Value) -> Option<String> {
    if let Some(text) = value.as_str() {
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }

    if let Some(int_value) = value.as_i64() {
        return Some(int_value.to_string());
    }

    if let Some(uint_value) = value.as_u64() {
        return Some(uint_value.to_string());
    }

    if let Some(float_value) = value.as_f64() {
        return Some(float_value.to_string());
    }

    None
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
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{\"root\":{\"state\":\"running\"},\"sub\":{\"state\":\"idle\",\"parentID\":\"root\"}}".to_string(),
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
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n[{\"id\":\"root\",\"directory\":\"/repo\",\"title\":\"Root Session\"},{\"id\":\"sub\",\"directory\":\"/repo\",\"parentID\":\"root\",\"title\":\"Sub Session\"}]".to_string(),
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
        assert_eq!(root.title.as_deref(), Some("Root Session"));

        let sub = records
            .iter()
            .find(|record| record.session_id == "sub")
            .expect("sub record should exist");
        assert_eq!(sub.parent_session_id.as_deref(), Some("root"));
        assert_eq!(sub.title.as_deref(), Some("Sub Session"));
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

    #[tokio::test(flavor = "multi_thread")]
    async fn fetch_session_messages_parses_message_array() {
        let port = spawn_single_response_server(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n[{\"role\":\"user\",\"content\":\"hello\",\"createdAt\":\"2026-01-01T00:00:00Z\"},{\"type\":\"assistant\",\"message\":{\"text\":\"world\"}}]".to_string(),
        )
        .await;
        let provider = ServerStatusProvider::new(ServerStatusConfig {
            port,
            request_timeout: Duration::from_millis(500),
            ..ServerStatusConfig::default()
        });

        let messages = provider
            .fetch_session_messages("sid-1")
            .await
            .expect("message response should parse");

        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].message_type.as_deref(), Some("text"));
        assert_eq!(messages[0].role.as_deref(), Some("user"));
        assert_eq!(messages[0].content, "hello");
        assert_eq!(
            messages[0].timestamp.as_deref(),
            Some("2026-01-01T00:00:00Z")
        );
        assert_eq!(messages[1].role.as_deref(), Some("assistant"));
        assert_eq!(messages[1].content, "world");
        assert_eq!(messages[1].message_type.as_deref(), Some("text"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn fetch_session_messages_parses_messages_wrapper() {
        let port = spawn_single_response_server(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{\"messages\":[{\"author\":{\"role\":\"assistant\"},\"parts\":[{\"text\":\"first\"},{\"text\":\"second\"}]}]}".to_string(),
        )
        .await;
        let provider = ServerStatusProvider::new(ServerStatusConfig {
            port,
            request_timeout: Duration::from_millis(500),
            ..ServerStatusConfig::default()
        });

        let messages = provider
            .fetch_session_messages("sid-2")
            .await
            .expect("wrapped message response should parse");

        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].role.as_deref(), Some("assistant"));
        assert_eq!(messages[0].content, "first\nsecond");
        assert_eq!(messages[0].message_type.as_deref(), Some("text"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn fetch_session_messages_skips_non_text_entries() {
        let port = spawn_single_response_server(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n[{\"info\":{\"role\":\"assistant\"},\"parts\":[{\"type\":\"step-start\",\"title\":\"Planning\"}]},{\"info\":{\"role\":\"assistant\"},\"parts\":[{\"type\":\"text\",\"text\":\"real output\"}]}]".to_string(),
        )
        .await;
        let provider = ServerStatusProvider::new(ServerStatusConfig {
            port,
            request_timeout: Duration::from_millis(500),
            ..ServerStatusConfig::default()
        });

        let messages = provider
            .fetch_session_messages("sid-3")
            .await
            .expect("message response should parse while skipping non-text entries");

        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].role.as_deref(), Some("assistant"));
        assert_eq!(messages[0].content, "real output");
        assert_eq!(messages[0].message_type.as_deref(), Some("text"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn fetch_session_messages_extracts_nested_numeric_timestamps() {
        let port = spawn_single_response_server(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n[{\"info\":{\"role\":\"assistant\",\"time\":{\"created\":1735689600}},\"parts\":[{\"type\":\"step-start\",\"time\":{\"start\":1735689601}},{\"type\":\"text\",\"text\":\"done\"}]}]".to_string(),
        )
        .await;
        let provider = ServerStatusProvider::new(ServerStatusConfig {
            port,
            request_timeout: Duration::from_millis(500),
            ..ServerStatusConfig::default()
        });

        let messages = provider
            .fetch_session_messages("sid-4")
            .await
            .expect("nested timestamp response should parse");

        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].role.as_deref(), Some("assistant"));
        assert_eq!(messages[0].content, "done");
        assert_eq!(messages[0].timestamp.as_deref(), Some("1735689600"));
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
