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
use crate::types::{SessionMessageItem, SessionStatusSource, SessionTodoItem};

/// Spawn a background task that polls task status from the OpenCode server
pub fn spawn_status_poller(
    db_path: PathBuf,
    stop: Arc<AtomicBool>,
    session_todo_cache: Arc<Mutex<HashMap<Uuid, Vec<SessionTodoItem>>>>,
    session_title_cache: Arc<Mutex<HashMap<String, String>>>,
    session_message_cache: Arc<Mutex<HashMap<Uuid, Vec<SessionMessageItem>>>>,
    poll_interval_ms: u64,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let server_provider = ServerStatusProvider::default();
        let db = loop {
            if stop.load(Ordering::Relaxed) {
                return;
            }

            match Database::open_async(&db_path).await {
                Ok(db) => break db,
                Err(err) => {
                    tracing::warn!(error = %err, "failed to open database for status poller");
                    interruptible_sleep(Duration::from_millis(poll_interval_ms), &stop).await;
                }
            }
        };

        while !stop.load(Ordering::Relaxed) {
            let tasks = match db.list_tasks_async().await {
                Ok(tasks) => tasks,
                Err(_) => {
                    interruptible_sleep(Duration::from_millis(poll_interval_ms), &stop).await;
                    continue;
                }
            };
            if tasks.is_empty() {
                interruptible_sleep(Duration::from_millis(poll_interval_ms), &stop).await;
                continue;
            }

            let repo_paths: HashMap<Uuid, String> = match db.list_repos_async().await {
                Ok(repos) => repos.into_iter().map(|repo| (repo.id, repo.path)).collect(),
                Err(err) => {
                    tracing::warn!(error = %err, "failed to list repos for status poller");
                    interruptible_sleep(Duration::from_millis(poll_interval_ms), &stop).await;
                    continue;
                }
            };
            let fetched_at = SystemTime::now();
            let session_records = server_provider.list_all_session_records().await.ok();
            let complete_session_parent_map = session_records.as_ref().map(|records| {
                records
                    .iter()
                    .map(|record| (record.session_id.clone(), record.parent_session_id.clone()))
                    .collect::<HashMap<_, _>>()
            });
            let mut next_todo_cache: HashMap<Uuid, Vec<SessionTodoItem>> = session_todo_cache
                .lock()
                .ok()
                .map(|cache| cache.clone())
                .unwrap_or_default();
            let mut next_message_cache: HashMap<Uuid, Vec<SessionMessageItem>> =
                session_message_cache
                    .lock()
                    .ok()
                    .map(|cache| cache.clone())
                    .unwrap_or_default();
            let mut next_title_cache: HashMap<String, String> = session_title_cache
                .lock()
                .ok()
                .map(|cache| cache.clone())
                .unwrap_or_default();
            if let Some(records) = session_records.as_ref() {
                next_title_cache.clear();
                for record in records {
                    if let Some(title) = record.title.as_ref() {
                        next_title_cache.insert(record.session_id.clone(), title.clone());
                    }
                }
            }

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

                let mut todo_session_id = task.opencode_session_id.clone();

                if !repo_available {
                    if task.tmux_status != Status::Idle.as_str() {
                        let _ = db
                            .update_task_status_async(task.id, Status::Idle.as_str())
                            .await;
                    }
                    debug!(
                        task_id = %task.id,
                        session_id = ?todo_session_id,
                        "repository unavailable; still attempting todo fetch"
                    );
                }

                if repo_available {
                    if let Some(worktree_path) = task.worktree_path.as_deref() {
                        debug!("Fetching status for task {} at {}", task.id, worktree_path);
                        let mut bound_session_id = task.opencode_session_id.clone();

                        match server_provider
                            .fetch_status_matches(fetched_at, Some(worktree_path))
                            .await
                        {
                            Ok(statuses) => {
                                debug!("Got {} statuses for task {}", statuses.len(), task.id);
                                if let Some(status_match) = select_status_match(
                                    statuses,
                                    complete_session_parent_map.as_ref(),
                                ) {
                                    debug!(
                                        "Task {} matched to session {} with status {:?}",
                                        task.id, status_match.session_id, status_match.status.state
                                    );

                                    if task.tmux_status != status_match.status.state.as_str() {
                                        let _ = db
                                            .update_task_status_async(
                                                task.id,
                                                status_match.status.state.as_str(),
                                            )
                                            .await;
                                    }

                                    if task.status_source != SessionStatusSource::Server.as_str()
                                        || task.status_error.is_some()
                                    {
                                        let _ = db
                                            .update_task_status_metadata_async(
                                                task.id,
                                                SessionStatusSource::Server.as_str(),
                                                Some(to_iso8601(fetched_at)),
                                                None,
                                            )
                                            .await;
                                    }

                                    if bound_session_id.as_deref()
                                        != Some(status_match.session_id.as_str())
                                    {
                                        let _ = db
                                            .update_task_session_binding_async(
                                                task.id,
                                                Some(status_match.session_id.clone()),
                                            )
                                            .await;
                                    }
                                    bound_session_id = Some(status_match.session_id);
                                    todo_session_id = bound_session_id.clone();
                                } else {
                                    debug!(
                                        "No active session for task {} - setting status to idle",
                                        task.id
                                    );
                                    let missing_id = task
                                        .tmux_session_name
                                        .clone()
                                        .unwrap_or_else(|| task.id.to_string());
                                    let missing_error = format!("SESSION_NOT_FOUND:{missing_id}");

                                    if task.tmux_status != Status::Idle.as_str() {
                                        let _ = db
                                            .update_task_status_async(
                                                task.id,
                                                Status::Idle.as_str(),
                                            )
                                            .await;
                                    }

                                    if task.status_source != SessionStatusSource::None.as_str()
                                        || task.status_error.as_deref()
                                            != Some(missing_error.as_str())
                                    {
                                        let _ = db
                                            .update_task_status_metadata_async(
                                                task.id,
                                                SessionStatusSource::None.as_str(),
                                                Some(to_iso8601(fetched_at)),
                                                Some(missing_error),
                                            )
                                            .await;
                                    }
                                }
                            }
                            Err(err) => {
                                tracing::warn!(
                                    "Failed to fetch status for task {} - marking status idle: {:?}",
                                    task.id,
                                    err
                                );
                                let error_text = format!("{}:{}", err.code, err.message);
                                if task.tmux_status != Status::Idle.as_str() {
                                    let _ = db
                                        .update_task_status_async(task.id, Status::Idle.as_str())
                                        .await;
                                }

                                if task.status_source != SessionStatusSource::None.as_str()
                                    || task.status_error.as_deref() != Some(error_text.as_str())
                                {
                                    let _ = db
                                        .update_task_status_metadata_async(
                                            task.id,
                                            SessionStatusSource::None.as_str(),
                                            Some(to_iso8601(fetched_at)),
                                            Some(error_text),
                                        )
                                        .await;
                                }
                                debug!(
                                    task_id = %task.id,
                                    session_id = ?todo_session_id,
                                    "status fetch failed; still attempting todo fetch"
                                );
                            }
                        }
                    } else {
                        debug!(
                            task_id = %task.id,
                            session_id = ?todo_session_id,
                            "task has no worktree path; still attempting todo fetch"
                        );
                    }
                }

                if let Some(session_id) = todo_session_id.as_deref() {
                    if let Some(todos) =
                        fetch_task_todos(&server_provider, task.id, Some(session_id)).await
                    {
                        debug!(
                            task_id = %task.id,
                            session_id,
                            todo_count = todos.len(),
                            poll_interval_ms,
                            "updated task todos from OpenCode server"
                        );
                        next_todo_cache.insert(task.id, todos);
                    } else {
                        debug!(
                            task_id = %task.id,
                            session_id,
                            "todo fetch failed; preserving previous cached todos"
                        );
                    }

                    if let Some(messages) =
                        fetch_task_messages(&server_provider, task.id, Some(session_id)).await
                    {
                        debug!(
                            task_id = %task.id,
                            session_id,
                            message_count = messages.len(),
                            poll_interval_ms,
                            "updated task messages from OpenCode server"
                        );
                        next_message_cache.insert(task.id, messages);
                    } else {
                        debug!(
                            task_id = %task.id,
                            session_id,
                            "message fetch failed; preserving previous cached messages"
                        );
                    }
                } else {
                    next_todo_cache.remove(&task.id);
                    next_message_cache.remove(&task.id);
                    debug!(
                        task_id = %task.id,
                        "no bound session; removed cached todos and messages"
                    );
                }

                if stop.load(Ordering::Relaxed) {
                    break;
                }
            }

            if stop.load(Ordering::Relaxed) {
                break;
            }

            if let Ok(mut cache) = session_todo_cache.lock() {
                cache.clear();
                cache.extend(next_todo_cache);
            }

            if let Ok(mut cache) = session_title_cache.lock() {
                cache.clear();
                cache.extend(next_title_cache);
            }

            if let Ok(mut cache) = session_message_cache.lock() {
                cache.clear();
                cache.extend(next_message_cache);
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

async fn fetch_task_messages(
    server_provider: &ServerStatusProvider,
    task_id: Uuid,
    session_id: Option<&str>,
) -> Option<Vec<SessionMessageItem>> {
    let Some(session_id) = session_id else {
        debug!(
            "Skipping message sync for task {} because no OpenCode session is bound",
            task_id
        );
        return None;
    };

    match server_provider.fetch_session_messages(session_id).await {
        Ok(messages) => Some(messages),
        Err(err) => {
            tracing::warn!(
                "Failed to fetch message list for task {} session {}: {:?}",
                task_id,
                session_id,
                err
            );
            None
        }
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
    let first = status_matches.first()?;
    let status_map: HashMap<String, &SessionStatusMatch> = status_matches
        .iter()
        .map(|m| (m.session_id.clone(), m))
        .collect();

    let mut parent_map: HashMap<String, Option<String>> = status_matches
        .iter()
        .map(|m| (m.session_id.clone(), m.parent_session_id.clone()))
        .collect();

    if let Some(complete_parent_map) = complete_parent_map {
        for (session_id, parent_session_id) in complete_parent_map {
            match parent_map.get_mut(session_id) {
                Some(existing_parent) => {
                    if existing_parent.is_none()
                        || (parent_session_id.is_some()
                            && existing_parent.as_ref() != parent_session_id.as_ref())
                    {
                        *existing_parent = parent_session_id.clone();
                    }
                }
                None => {
                    parent_map.insert(session_id.clone(), parent_session_id.clone());
                }
            }
        }
    }

    let mut selected_match = first;
    let mut selected_eldest = find_eldest_ancestor(first, &parent_map);
    for status_match in status_matches.iter().skip(1) {
        let candidate_eldest = find_eldest_ancestor(status_match, &parent_map);
        if candidate_eldest.1 > selected_eldest.1 {
            selected_match = status_match;
            selected_eldest = candidate_eldest;
        }
    }

    let eldest_id = selected_eldest.0;
    if let Some(eldest) = status_map.get(eldest_id.as_str()) {
        return Some((*eldest).clone());
    }

    Some(SessionStatusMatch {
        session_id: eldest_id,
        parent_session_id: None,
        status: selected_match.status.clone(),
    })
}

fn find_eldest_ancestor<'a>(
    session: &'a SessionStatusMatch,
    parent_map: &'a HashMap<String, Option<String>>,
) -> (String, usize) {
    let mut current = session.session_id.clone();
    let mut visited = HashSet::from([current.clone()]);
    let mut depth = 0;

    loop {
        let Some(parent_id) = parent_map
            .get(current.as_str())
            .and_then(|parent| parent.as_ref())
        else {
            return (current, depth);
        };

        if !visited.insert(parent_id.clone()) {
            return (current, depth);
        }

        current = parent_id.clone();
        depth += 1;
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

    #[test]
    fn select_status_match_prefers_longest_ancestor_chain() {
        let selected = select_status_match(
            vec![
                status_match("orphan-1", None),
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
    fn select_status_match_complete_map_overrides_incorrect_parent_links() {
        let mut parent_map = HashMap::new();
        parent_map.insert("subagent-1".to_string(), Some("middle-1".to_string()));
        parent_map.insert("middle-1".to_string(), Some("root-1".to_string()));
        parent_map.insert("root-1".to_string(), None);

        let selected = select_status_match(
            vec![status_match("subagent-1", Some("stale-parent"))],
            Some(&parent_map),
        )
        .expect("expected a selected match");

        assert_eq!(selected.session_id, "root-1");
        assert!(selected.parent_session_id.is_none());
    }
}
