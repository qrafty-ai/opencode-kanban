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
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
pub enum SessionState {
    Running,
    Waiting,
    Idle,
    Dead,
}

impl SessionState {
    pub fn as_str(self) -> &'static str {
        match self {
            SessionState::Running => "running",
            SessionState::Waiting => "waiting",
            SessionState::Idle => "idle",
            SessionState::Dead => "dead",
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
