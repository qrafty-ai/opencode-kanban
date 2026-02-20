#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub struct Repo {
    pub id: Uuid,
    pub path: String,
    pub name: String,
    pub default_base: Option<String>,
    pub remote_url: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub struct Category {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub position: i64,
    pub color: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub struct Task {
    pub id: Uuid,
    pub title: String,
    pub repo_id: Uuid,
    pub branch: String,
    pub category_id: Uuid,
    pub position: i64,
    pub tmux_session_name: Option<String>,
    pub worktree_path: Option<String>,
    pub tmux_status: String,
    pub status_source: String,
    pub status_fetched_at: Option<String>,
    pub status_error: Option<String>,
    pub opencode_session_id: Option<String>,
    #[serde(default)]
    pub attach_overlay_shown: bool,
    pub archived: bool,
    pub archived_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub struct SessionTodoItem {
    pub content: String,
    pub completed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub struct SessionMessageItem {
    pub message_type: Option<String>,
    pub role: Option<String>,
    pub content: String,
    pub timestamp: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
pub enum SessionState {
    Running,
    Idle,
}

impl SessionState {
    pub fn as_str(self) -> &'static str {
        match self {
            SessionState::Running => "running",
            SessionState::Idle => "idle",
        }
    }

    pub fn from_raw_status(raw: &str) -> Self {
        let normalized = raw.trim().to_ascii_lowercase();
        match normalized.as_str() {
            "running" | "active" | "thinking" | "processing" | "busy" => SessionState::Running,
            _ => SessionState::Idle,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
pub enum SessionStatusSource {
    Server,
    None,
}

impl SessionStatusSource {
    pub fn as_str(self) -> &'static str {
        match self {
            SessionStatusSource::Server => "server",
            SessionStatusSource::None => "none",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub struct SessionStatusError {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub struct SessionStatus {
    pub state: SessionState,
    pub source: SessionStatusSource,
    pub fetched_at: std::time::SystemTime,
    pub error: Option<SessionStatusError>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub struct CommandFrequency {
    pub command_id: String,
    pub use_count: i64,
    pub last_used: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_state_as_str() {
        assert_eq!(SessionState::Running.as_str(), "running");
        assert_eq!(SessionState::Idle.as_str(), "idle");
    }

    #[test]
    fn test_session_state_from_raw_status_running() {
        assert_eq!(
            SessionState::from_raw_status("running"),
            SessionState::Running
        );
        assert_eq!(
            SessionState::from_raw_status("  Running  "),
            SessionState::Running
        );
        assert_eq!(
            SessionState::from_raw_status("active"),
            SessionState::Running
        );
        assert_eq!(
            SessionState::from_raw_status("thinking"),
            SessionState::Running
        );
        assert_eq!(
            SessionState::from_raw_status("processing"),
            SessionState::Running
        );
        assert_eq!(SessionState::from_raw_status("BUSY"), SessionState::Running);
    }

    #[test]
    fn test_session_state_from_raw_status_idle() {
        assert_eq!(SessionState::from_raw_status("idle"), SessionState::Idle);
        assert_eq!(
            SessionState::from_raw_status("  Idle  "),
            SessionState::Idle
        );
        assert_eq!(
            SessionState::from_raw_status("completed"),
            SessionState::Idle
        );
        assert_eq!(SessionState::from_raw_status("stopped"), SessionState::Idle);
        assert_eq!(SessionState::from_raw_status(""), SessionState::Idle);
    }

    #[test]
    fn test_session_status_source_as_str() {
        assert_eq!(SessionStatusSource::Server.as_str(), "server");
        assert_eq!(SessionStatusSource::None.as_str(), "none");
    }

    #[test]
    fn test_repo_struct_creation() {
        let repo = Repo {
            id: Uuid::new_v4(),
            path: "/path/to/repo".to_string(),
            name: "test-repo".to_string(),
            default_base: Some("main".to_string()),
            remote_url: Some("https://github.com/test/repo.git".to_string()),
            created_at: "2024-01-01".to_string(),
            updated_at: "2024-01-02".to_string(),
        };
        assert_eq!(repo.name, "test-repo");
        assert_eq!(repo.path, "/path/to/repo");
    }

    #[test]
    fn test_category_struct_creation() {
        let category = Category {
            id: Uuid::new_v4(),
            slug: "todo".to_string(),
            name: "To Do".to_string(),
            position: 0,
            color: Some("#FF0000".to_string()),
            created_at: "2024-01-01".to_string(),
        };
        assert_eq!(category.slug, "todo");
        assert_eq!(category.name, "To Do");
    }

    #[test]
    fn test_task_struct_creation() {
        let task = Task {
            id: Uuid::new_v4(),
            title: "Test Task".to_string(),
            repo_id: Uuid::new_v4(),
            branch: "feature/test".to_string(),
            category_id: Uuid::new_v4(),
            position: 0,
            tmux_session_name: Some("session-1".to_string()),
            worktree_path: Some("/path/to/worktree".to_string()),
            tmux_status: "idle".to_string(),
            status_source: "none".to_string(),
            status_fetched_at: Some("2024-01-01".to_string()),
            status_error: None,
            opencode_session_id: Some("sess-123".to_string()),
            archived: false,
            archived_at: None,
            created_at: "2024-01-01".to_string(),
            updated_at: "2024-01-02".to_string(),
        };
        assert_eq!(task.title, "Test Task");
        assert!(!task.archived);
    }

    #[test]
    fn test_session_todo_item_struct() {
        let item = SessionTodoItem {
            content: "Test todo".to_string(),
            completed: true,
        };
        assert_eq!(item.content, "Test todo");
        assert!(item.completed);
    }

    #[test]
    fn test_session_message_item_struct() {
        let item = SessionMessageItem {
            message_type: Some("text".to_string()),
            role: Some("user".to_string()),
            content: "Hello".to_string(),
            timestamp: Some("2024-01-01".to_string()),
        };
        assert_eq!(item.content, "Hello");
        assert_eq!(item.role, Some("user".to_string()));
    }

    #[test]
    fn test_session_status_error_struct() {
        let error = SessionStatusError {
            code: "404".to_string(),
            message: "Not found".to_string(),
        };
        assert_eq!(error.code, "404");
        assert_eq!(error.message, "Not found");
    }

    #[test]
    fn test_command_frequency_struct() {
        let freq = CommandFrequency {
            command_id: "cmd-1".to_string(),
            use_count: 42,
            last_used: "2024-01-01".to_string(),
        };
        assert_eq!(freq.command_id, "cmd-1");
        assert_eq!(freq.use_count, 42);
    }
}
