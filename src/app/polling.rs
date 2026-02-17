//! Status polling for async task status updates

use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use chrono::{DateTime, Utc};
use tracing::debug;
use uuid::Uuid;

use crate::db::Database;
use crate::opencode::status_server::SessionStatusMatch;
use crate::opencode::{ServerStatusProvider, Status};
use crate::types::SessionStatusSource;

use super::state::STATUS_REPO_UNAVAILABLE;

/// Spawn a background thread that polls task status from the OpenCode server
pub fn spawn_status_poller(
    db_path: PathBuf,
    stop: Arc<AtomicBool>,
    poll_interval_ms: u64,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let server_provider = ServerStatusProvider::default();

        while !stop.load(Ordering::Relaxed) {
            let db = match Database::open(&db_path) {
                Ok(db) => db,
                Err(_) => {
                    interruptible_sleep(Duration::from_millis(poll_interval_ms), &stop);
                    continue;
                }
            };

            let tasks = db.list_tasks().unwrap_or_default();
            if tasks.is_empty() {
                interruptible_sleep(Duration::from_millis(poll_interval_ms), &stop);
                continue;
            }

            let repos = db.list_repos().unwrap_or_default();
            let repo_paths: HashMap<Uuid, String> =
                repos.into_iter().map(|repo| (repo.id, repo.path)).collect();
            let fetched_at = SystemTime::now();

            for (index, task) in tasks.iter().enumerate() {
                if stop.load(Ordering::Relaxed) {
                    break;
                }

                let repo_available = repo_paths
                    .get(&task.repo_id)
                    .map(|path| Path::new(path).exists())
                    .unwrap_or(false);

                if !repo_available {
                    let _ = db.update_task_status(task.id, STATUS_REPO_UNAVAILABLE);
                    let _ = db.update_task_todo(task.id, None);
                    interruptible_sleep(staggered_poll_delay(index, poll_interval_ms), &stop);
                    continue;
                }

                if let Some(worktree_path) = task.worktree_path.as_deref() {
                    debug!("Fetching status for task {} at {}", task.id, worktree_path);
                    let mut bound_session_id = task.opencode_session_id.clone();

                    match server_provider.fetch_status_matches(fetched_at, Some(worktree_path)) {
                        Ok(statuses) => {
                            debug!("Got {} statuses for task {}", statuses.len(), task.id);
                            if let Some(status_match) = select_status_match(statuses) {
                                debug!(
                                    "Task {} matched to session {} with status {:?}",
                                    task.id, status_match.session_id, status_match.status.state
                                );

                                let _ = db.update_task_status(
                                    task.id,
                                    status_match.status.state.as_str(),
                                );
                                let _ = db.update_task_status_metadata(
                                    task.id,
                                    SessionStatusSource::Server.as_str(),
                                    Some(to_iso8601(fetched_at)),
                                    None,
                                );

                                if bound_session_id.as_deref()
                                    != Some(status_match.session_id.as_str())
                                {
                                    let _ = db.update_task_session_binding(
                                        task.id,
                                        Some(status_match.session_id.clone()),
                                    );
                                }
                                bound_session_id = Some(status_match.session_id);
                            } else {
                                debug!(
                                    "No active session for task {} - setting status to dead",
                                    task.id
                                );
                                let _ = db.update_task_status(task.id, Status::Dead.as_str());
                                let missing_id = task
                                    .tmux_session_name
                                    .clone()
                                    .unwrap_or_else(|| task.id.to_string());
                                let _ = db.update_task_status_metadata(
                                    task.id,
                                    SessionStatusSource::None.as_str(),
                                    Some(to_iso8601(fetched_at)),
                                    Some(format!("SESSION_NOT_FOUND:{missing_id}")),
                                );
                            }

                            update_task_todos(
                                &db,
                                &server_provider,
                                task.id,
                                bound_session_id.as_deref(),
                            );
                        }
                        Err(err) => {
                            tracing::warn!(
                                "Failed to fetch status for task {} - marking status dead: {:?}",
                                task.id,
                                err
                            );
                            let _ = db.update_task_status(task.id, Status::Dead.as_str());
                            let _ = db.update_task_status_metadata(
                                task.id,
                                SessionStatusSource::None.as_str(),
                                Some(to_iso8601(fetched_at)),
                                Some(format!("{}:{}", err.code, err.message)),
                            );
                            let _ = db.update_task_todo(task.id, None);
                        }
                    }
                }

                interruptible_sleep(staggered_poll_delay(index, poll_interval_ms), &stop);
            }
        }
    })
}

fn update_task_todos(
    db: &Database,
    server_provider: &ServerStatusProvider,
    task_id: Uuid,
    session_id: Option<&str>,
) {
    let Some(session_id) = session_id else {
        debug!(
            "Skipping todo sync for task {} because no OpenCode session is bound",
            task_id
        );
        return;
    };

    match server_provider.fetch_session_todo(session_id) {
        Ok(todos) => match serde_json::to_string(&todos) {
            Ok(raw) => {
                let _ = db.update_task_todo(task_id, Some(raw));
            }
            Err(err) => {
                tracing::warn!(
                    "Failed to serialize todo payload for task {} session {}: {}",
                    task_id,
                    session_id,
                    err
                );
                let _ = db.update_task_todo(task_id, None);
            }
        },
        Err(err) => {
            tracing::warn!(
                "Failed to fetch todo list for task {} session {}: {:?}",
                task_id,
                session_id,
                err
            );
            let _ = db.update_task_todo(task_id, None);
        }
    }
}

/// Convert SystemTime to ISO 8601 string
fn to_iso8601(time: SystemTime) -> String {
    DateTime::<Utc>::from(time).to_rfc3339()
}

/// Calculate staggered poll delay to avoid thundering herd
pub fn staggered_poll_delay(task_index: usize, base_poll_interval_ms: u64) -> Duration {
    let base_ms = base_poll_interval_ms.saturating_mul(1 + task_index as u64);
    let jitter_ms = current_jitter_ms(task_index);
    Duration::from_millis(base_ms) + Duration::from_millis(jitter_ms)
}

/// Generate jitter based on current time and task index
fn current_jitter_ms(task_index: usize) -> u64 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as u64)
        .unwrap_or(0);
    (nanos + task_index as u64 * 97) % 700
}

/// Sleep that can be interrupted by stop signal
fn interruptible_sleep(duration: Duration, stop: &AtomicBool) {
    let chunk = Duration::from_millis(100);
    let mut remaining = duration;
    while remaining > Duration::ZERO && !stop.load(Ordering::Relaxed) {
        let sleep_duration = remaining.min(chunk);
        thread::sleep(sleep_duration);
        remaining = remaining.saturating_sub(sleep_duration);
    }
}

fn select_status_match(status_matches: Vec<SessionStatusMatch>) -> Option<SessionStatusMatch> {
    if let Some(root) = status_matches.iter().find(|m| m.is_root_session()) {
        return Some(root.clone());
    }
    let session_map: HashMap<String, &SessionStatusMatch> = status_matches
        .iter()
        .map(|m| (m.session_id.clone(), m))
        .collect();
    let first = status_matches.first()?;
    Some(find_eldest_ancestor(first, &session_map))
}

fn find_eldest_ancestor<'a>(
    session: &'a SessionStatusMatch,
    session_map: &'a HashMap<String, &'a SessionStatusMatch>,
) -> SessionStatusMatch {
    if let Some(parent_id) = &session.parent_session_id {
        if let Some(parent) = session_map.get(parent_id) {
            return find_eldest_ancestor(parent, session_map);
        }
        return SessionStatusMatch {
            session_id: parent_id.clone(),
            parent_session_id: None,
            status: session.status.clone(),
        };
    }
    session.clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{SessionState, SessionStatus};

    fn status_match(session_id: &str, parent_session_id: Option<&str>) -> SessionStatusMatch {
        SessionStatusMatch {
            session_id: session_id.to_string(),
            parent_session_id: parent_session_id.map(str::to_string),
            status: SessionStatus {
                state: SessionState::Running,
                source: SessionStatusSource::Server,
                fetched_at: SystemTime::UNIX_EPOCH,
                error: None,
            },
        }
    }

    #[test]
    fn select_status_match_prefers_root_session() {
        let selected = select_status_match(vec![
            status_match("subagent-1", Some("root-1")),
            status_match("root-1", None),
            status_match("subagent-2", Some("root-1")),
        ])
        .expect("expected a selected match");

        assert_eq!(selected.session_id, "root-1");
    }

    #[test]
    fn select_status_match_promotes_to_parent_if_no_root_found() {
        let selected = select_status_match(vec![
            status_match("subagent-1", Some("root-1")),
            status_match("subagent-2", Some("root-1")),
        ])
        .expect("expected a selected match");

        assert_eq!(selected.session_id, "root-1");
        assert!(selected.parent_session_id.is_none());
    }

    #[test]
    fn select_status_match_walks_chain_to_eldest_ancestor() {
        let selected = select_status_match(vec![
            status_match("subagent-1", Some("middle-1")),
            status_match("middle-1", Some("root-1")),
        ])
        .expect("expected a selected match");

        assert_eq!(selected.session_id, "root-1");
        assert!(selected.parent_session_id.is_none());
    }

    #[test]
    fn select_status_match_returns_none_for_empty_results() {
        assert!(select_status_match(Vec::new()).is_none());
    }
}
