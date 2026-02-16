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
    pub opencode_session_id: Option<String>,
    pub worktree_path: Option<String>,
    pub tmux_status: String,
    pub created_at: String,
    pub updated_at: String,
}
