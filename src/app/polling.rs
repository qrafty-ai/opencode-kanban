//! Status polling for async task status updates

use std::collections::HashMap;
use std::collections::HashSet;
use std::path::Path;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use chrono::{DateTime, Utc};
use tokio::task::JoinHandle;
use tracing::debug;
use uuid::Uuid;

use crate::db::Database;
use crate::opencode::status_server::SessionStatusMatch;
use crate::opencode::{ServerStatusProvider, Status};
use crate::types::{SessionStatusSource, SessionTodoItem};

use super::state::STATUS_REPO_UNAVAILABLE;

/// Spawn a background task that polls task status from the OpenCode server
pub fn spawn_status_poller(
    db_path: PathBuf,
    stop: Arc<AtomicBool>,
    session_todo_cache: Arc<Mutex<HashMap<Uuid, Vec<SessionTodoItem>>>>,
    poll_interval_ms: u64,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let server_provider = ServerStatusProvider::default();

        while !stop.load(Ordering::Relaxed) {
            let db = match Database::open(&db_path) {
                Ok(db) => db,
                Err(_) => {
                    interruptible_sleep(Duration::from_millis(poll_interval_ms), &stop).await;
                    continue;
                }
            };

            let tasks = db.list_tasks().unwrap_or_default();
            if tasks.is_empty() {
                interruptible_sleep(Duration::from_millis(poll_interval_ms), &stop).await;
                continue;
            }

            let repo_paths: HashMap<Uuid, String> = db
                .list_repos()
                .unwrap_or_default()
                .into_iter()
                .map(|repo| (repo.id, repo.path))
                .collect();
            drop(db);
            let fetched_at = SystemTime::now();
            let complete_session_parent_map = server_provider.fetch_session_parent_map().await.ok();

            debug!(
                poll_interval_ms,
                task_count = tasks.len(),
                "status/todo poll cycle started"
            );

            for task in &tasks {
                if stop.load(Ordering::Relaxed) {
                    break;
                }

                let repo_available = repo_paths
                    .get(&task.repo_id)
                    .map(|path| Path::new(path).exists())
                    .unwrap_or(false);

                if !repo_available {
                    if let Ok(db) = Database::open(&db_path) {
                        let _ = db.update_task_status(task.id, STATUS_REPO_UNAVAILABLE);
                    }
                    clear_task_todos(&session_todo_cache, task.id);
                    debug!(
                        task_id = %task.id,
                        "cleared cached todos because repository is unavailable"
                    );
                    continue;
                }

                if let Some(worktree_path) = task.worktree_path.as_deref() {
                    debug!("Fetching status for task {} at {}", task.id, worktree_path);
                    let mut bound_session_id = task.opencode_session_id.clone();

                    match server_provider
                        .fetch_status_matches(fetched_at, Some(worktree_path))
                        .await
                    {
                        Ok(statuses) => {
                            debug!("Got {} statuses for task {}", statuses.len(), task.id);
                            let mut todo_session_id: Option<String> = None;
                            if let Ok(db) = Database::open(&db_path) {
                                if let Some(status_match) = select_status_match(
                                    statuses,
                                    complete_session_parent_map.as_ref(),
                                ) {
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
                                    todo_session_id = bound_session_id.clone();
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
                            }

                            if let Some(todos) = fetch_task_todos(
                                &server_provider,
                                task.id,
                                todo_session_id.as_deref(),
                            )
                            .await
                            {
                                debug!(
                                    task_id = %task.id,
                                    session_id = ?todo_session_id,
                                    todo_count = todos.len(),
                                    poll_interval_ms,
                                    "updated task todos from OpenCode server"
                                );
                                set_task_todos(&session_todo_cache, task.id, todos);
                            } else {
                                clear_task_todos(&session_todo_cache, task.id);
                                debug!(
                                    task_id = %task.id,
                                    session_id = ?todo_session_id,
                                    "cleared cached todos because server todo fetch returned none"
                                );
                            }
                        }
                        Err(err) => {
                            tracing::warn!(
                                "Failed to fetch status for task {} - marking status dead: {:?}",
                                task.id,
                                err
                            );
                            if let Ok(db) = Database::open(&db_path) {
                                let _ = db.update_task_status(task.id, Status::Dead.as_str());
                                let _ = db.update_task_status_metadata(
                                    task.id,
                                    SessionStatusSource::None.as_str(),
                                    Some(to_iso8601(fetched_at)),
                                    Some(format!("{}:{}", err.code, err.message)),
                                );
                            }
                            clear_task_todos(&session_todo_cache, task.id);
                            debug!(
                                task_id = %task.id,
                                "cleared cached todos because status fetch failed"
                            );
                        }
                    }
                } else {
                    clear_task_todos(&session_todo_cache, task.id);
                    debug!(
                        task_id = %task.id,
                        "cleared cached todos because task has no worktree path"
                    );
                }

                if stop.load(Ordering::Relaxed) {
                    break;
                }
            }

            if stop.load(Ordering::Relaxed) {
                break;
            }

            debug!(
                poll_interval_ms,
                task_count = tasks.len(),
                "status/todo poll cycle complete; sleeping until next cycle"
            );
            interruptible_sleep(Duration::from_millis(poll_interval_ms), &stop).await;
        }
    })
}

async fn fetch_task_todos(
    server_provider: &ServerStatusProvider,
    task_id: Uuid,
    session_id: Option<&str>,
) -> Option<Vec<SessionTodoItem>> {
    let Some(session_id) = session_id else {
        debug!(
            "Skipping todo sync for task {} because no OpenCode session is bound",
            task_id
        );
        return None;
    };

    match server_provider.fetch_session_todo(session_id).await {
        Ok(todos) => Some(todos),
        Err(err) => {
            tracing::warn!(
                "Failed to fetch todo list for task {} session {}: {:?}",
                task_id,
                session_id,
                err
            );
            None
        }
    }
}

fn set_task_todos(
    session_todo_cache: &Arc<Mutex<HashMap<Uuid, Vec<SessionTodoItem>>>>,
    task_id: Uuid,
    todos: Vec<SessionTodoItem>,
) {
    if let Ok(mut cache) = session_todo_cache.lock() {
        cache.insert(task_id, todos);
    }
}

fn clear_task_todos(
    session_todo_cache: &Arc<Mutex<HashMap<Uuid, Vec<SessionTodoItem>>>>,
    task_id: Uuid,
) {
    if let Ok(mut cache) = session_todo_cache.lock() {
        cache.remove(&task_id);
    }
}

/// Convert SystemTime to ISO 8601 string
fn to_iso8601(time: SystemTime) -> String {
    DateTime::<Utc>::from(time).to_rfc3339()
}

/// Sleep that can be interrupted by stop signal
async fn interruptible_sleep(duration: Duration, stop: &AtomicBool) {
    let chunk = Duration::from_millis(100);
    let mut remaining = duration;
    while remaining > Duration::ZERO && !stop.load(Ordering::Relaxed) {
        let sleep_duration = remaining.min(chunk);
        tokio::time::sleep(sleep_duration).await;
        remaining = remaining.saturating_sub(sleep_duration);
    }
}

fn select_status_match(
    status_matches: Vec<SessionStatusMatch>,
    complete_parent_map: Option<&HashMap<String, Option<String>>>,
) -> Option<SessionStatusMatch> {
    let status_map: HashMap<String, &SessionStatusMatch> = status_matches
        .iter()
        .map(|m| (m.session_id.clone(), m))
        .collect();
    let first = status_matches.first()?;

    let mut parent_map: HashMap<String, Option<String>> = status_matches
        .iter()
        .map(|m| (m.session_id.clone(), m.parent_session_id.clone()))
        .collect();

    if let Some(complete_parent_map) = complete_parent_map {
        for (session_id, parent_session_id) in complete_parent_map {
            match parent_map.get_mut(session_id) {
                Some(existing_parent)
                    if existing_parent.is_none() && parent_session_id.is_some() =>
                {
                    *existing_parent = parent_session_id.clone();
                }
                Some(_) => {}
                None => {
                    parent_map.insert(session_id.clone(), parent_session_id.clone());
                }
            }
        }
    }

    let eldest_id = find_eldest_ancestor_id(first, &parent_map);
    if let Some(eldest) = status_map.get(eldest_id.as_str()) {
        return Some((*eldest).clone());
    }

    Some(SessionStatusMatch {
        session_id: eldest_id,
        parent_session_id: None,
        status: first.status.clone(),
    })
}

fn find_eldest_ancestor_id<'a>(
    session: &'a SessionStatusMatch,
    parent_map: &'a HashMap<String, Option<String>>,
) -> String {
    let mut current = session.session_id.clone();
    let mut visited = HashSet::from([current.clone()]);

    loop {
        let Some(parent_id) = parent_map
            .get(current.as_str())
            .and_then(|parent| parent.as_ref())
        else {
            return current;
        };

        if !visited.insert(parent_id.clone()) {
            return current;
        }

        current = parent_id.clone();
    }
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
        let selected = select_status_match(
            vec![
                status_match("subagent-1", Some("root-1")),
                status_match("root-1", None),
                status_match("subagent-2", Some("root-1")),
            ],
            None,
        )
        .expect("expected a selected match");

        assert_eq!(selected.session_id, "root-1");
    }

    #[test]
    fn select_status_match_promotes_to_parent_if_no_root_found() {
        let selected = select_status_match(
            vec![
                status_match("subagent-1", Some("root-1")),
                status_match("subagent-2", Some("root-1")),
            ],
            None,
        )
        .expect("expected a selected match");

        assert_eq!(selected.session_id, "root-1");
        assert!(selected.parent_session_id.is_none());
    }

    #[test]
    fn select_status_match_walks_chain_to_eldest_ancestor() {
        let selected = select_status_match(
            vec![
                status_match("subagent-1", Some("middle-1")),
                status_match("middle-1", Some("root-1")),
            ],
            None,
        )
        .expect("expected a selected match");

        assert_eq!(selected.session_id, "root-1");
        assert!(selected.parent_session_id.is_none());
    }

    #[test]
    fn select_status_match_returns_none_for_empty_results() {
        assert!(select_status_match(Vec::new(), None).is_none());
    }

    #[test]
    fn select_status_match_uses_complete_parent_map_to_find_eldest() {
        let mut parent_map = HashMap::new();
        parent_map.insert("subagent-1".to_string(), Some("middle-1".to_string()));
        parent_map.insert("middle-1".to_string(), Some("root-1".to_string()));
        parent_map.insert("root-1".to_string(), None);

        let selected = select_status_match(
            vec![status_match("subagent-1", Some("middle-1"))],
            Some(&parent_map),
        )
        .expect("expected a selected match");

        assert_eq!(selected.session_id, "root-1");
        assert!(selected.parent_session_id.is_none());
    }

    #[test]
    fn select_status_match_complete_map_overrides_missing_parent_links() {
        let mut parent_map = HashMap::new();
        parent_map.insert("middle-1".to_string(), Some("root-1".to_string()));
        parent_map.insert("root-1".to_string(), None);

        let selected = select_status_match(
            vec![
                status_match("subagent-1", Some("middle-1")),
                status_match("middle-1", None),
            ],
            Some(&parent_map),
        )
        .expect("expected a selected match");

        assert_eq!(selected.session_id, "root-1");
        assert!(selected.parent_session_id.is_none());
    }
}
