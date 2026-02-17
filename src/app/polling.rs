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
pub fn spawn_status_poller(db_path: PathBuf, stop: Arc<AtomicBool>) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let server_provider = ServerStatusProvider::default();

        while !stop.load(Ordering::Relaxed) {
            let db = match Database::open(&db_path) {
                Ok(db) => db,
                Err(_) => {
                    interruptible_sleep(Duration::from_secs(1), &stop);
                    continue;
                }
            };

            let tasks = db.list_tasks().unwrap_or_default();
            if tasks.is_empty() {
                interruptible_sleep(Duration::from_secs(1), &stop);
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
                    interruptible_sleep(staggered_poll_delay(index), &stop);
                    continue;
                }

                if let Some(worktree_path) = task.worktree_path.as_deref() {
                    debug!("Fetching status for task {} at {}", task.id, worktree_path);
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
                                update_task_todos(&db, &server_provider, task.id, &status_match);
                            } else {
                                debug!(
                                    "No active session for task {} - setting status to idle",
                                    task.id
                                );
                                let _ = db.update_task_status(task.id, Status::Idle.as_str());
                                let _ = db.update_task_status_metadata(
                                    task.id,
                                    SessionStatusSource::Server.as_str(),
                                    Some(to_iso8601(fetched_at)),
                                    None,
                                );
                                let _ = db.update_task_todo(task.id, None);
                            }
                        }
                        Err(err) => {
                            tracing::warn!(
                                "Failed to fetch status for task {} - skipping status update: {:?}",
                                task.id,
                                err
                            );
                        }
                    }
                }

                interruptible_sleep(staggered_poll_delay(index), &stop);
            }
        }
    })
}

fn update_task_todos(
    db: &Database,
    server_provider: &ServerStatusProvider,
    task_id: Uuid,
    status_match: &SessionStatusMatch,
) {
    match server_provider.fetch_session_todo(&status_match.session_id) {
        Ok(todos) => match serde_json::to_string(&todos) {
            Ok(raw) => {
                let _ = db.update_task_todo(task_id, Some(raw));
            }
            Err(err) => {
                tracing::warn!(
                    "Failed to serialize todo payload for task {} session {}: {}",
                    task_id,
                    status_match.session_id,
                    err
                );
                let _ = db.update_task_todo(task_id, None);
            }
        },
        Err(err) => {
            tracing::warn!(
                "Failed to fetch todo list for task {} session {}: {:?}",
                task_id,
                status_match.session_id,
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
pub fn staggered_poll_delay(task_index: usize) -> Duration {
    let base_seconds = 1 + task_index as u64;
    let jitter_ms = current_jitter_ms(task_index);
    Duration::from_secs(base_seconds) + Duration::from_millis(jitter_ms)
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
    let mut matches = status_matches.into_iter();
    let first = matches.next()?;
    if first.is_root_session() {
        return Some(first);
    }

    matches
        .find(|status_match| status_match.is_root_session())
        .or(Some(first))
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
    fn select_status_match_falls_back_to_first_when_all_are_subagents() {
        let selected = select_status_match(vec![
            status_match("subagent-1", Some("root-1")),
            status_match("subagent-2", Some("root-1")),
        ])
        .expect("expected a selected match");

        assert_eq!(selected.session_id, "subagent-1");
    }

    #[test]
    fn select_status_match_returns_none_for_empty_results() {
        assert!(select_status_match(Vec::new()).is_none());
    }
}
