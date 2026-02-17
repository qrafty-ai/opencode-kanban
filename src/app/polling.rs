//! Status polling for async task status updates

use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use chrono::{DateTime, Utc};
use tracing::debug;
use uuid::Uuid;

use crate::db::Database;
use crate::opencode::{ServerStatusProvider, Status};
use crate::tmux::tmux_session_exists;
use crate::types::{SessionStatus, SessionStatusSource};

use super::state::STATUS_REPO_UNAVAILABLE;

static LATEST_SESSION_SNAPSHOT: LazyLock<Mutex<HashMap<String, SessionStatus>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static LATEST_TASK_ROOT_BINDINGS: LazyLock<Mutex<HashMap<Uuid, String>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub fn latest_session_snapshot() -> HashMap<String, SessionStatus> {
    LATEST_SESSION_SNAPSHOT.lock().unwrap().clone()
}

pub fn latest_task_root_bindings() -> HashMap<Uuid, String> {
    LATEST_TASK_ROOT_BINDINGS.lock().unwrap().clone()
}

/// Spawn a background thread that polls task status from the OpenCode server
pub fn spawn_status_poller(db_path: PathBuf, stop: Arc<AtomicBool>) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let runtime = match tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
        {
            Ok(runtime) => runtime,
            Err(_) => return,
        };

        runtime.block_on(async move {
            while !stop.load(Ordering::Relaxed) {
                let db = match Database::open(&db_path) {
                    Ok(db) => db,
                    Err(_) => {
                        interruptible_sleep(Duration::from_secs(1), &stop).await;
                        continue;
                    }
                };

                let tasks = db.list_tasks().unwrap_or_default();
                if tasks.is_empty() {
                    interruptible_sleep(Duration::from_secs(1), &stop).await;
                    continue;
                }

                let repos = db.list_repos().unwrap_or_default();
                let repo_paths: HashMap<Uuid, String> =
                    repos.into_iter().map(|repo| (repo.id, repo.path)).collect();
                let server_provider = ServerStatusProvider::default();

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
                        interruptible_sleep(staggered_poll_delay(index), &stop).await;
                        continue;
                    }

                    if let Some(worktree_path) = task.worktree_path.as_deref() {
                        debug!(
                            "Fetching status for task {} at {}",
                            task.id, worktree_path
                        );
                        match server_provider.fetch_all_statuses(fetched_at, Some(worktree_path)) {
                            Ok(statuses) => {
                                debug!(
                                    "Got {} statuses for task {}",
                                    statuses.len(),
                                    task.id
                                );

                                if let Ok(mut snapshot) = LATEST_SESSION_SNAPSHOT.lock() {
                                    for (id, status) in &statuses {
                                        snapshot.insert(id.clone(), status.clone());
                                    }
                                }

                                if let Some((session_id, session_status)) =
                                    statuses.iter().next()
                                {
                                    debug!(
                                        "Task {} matched to session {} with status {:?}",
                                        task.id, session_id, session_status.state
                                    );

                                    if let Ok(mut bindings) = LATEST_TASK_ROOT_BINDINGS.lock() {
                                        bindings.insert(task.id, session_id.clone());
                                    }

                                    let _ = db
                                        .update_task_status(task.id, session_status.state.as_str());
                                    let _ = db.update_task_status_metadata(
                                        task.id,
                                        SessionStatusSource::Server.as_str(),
                                        Some(to_iso8601(fetched_at)),
                                        None,
                                    );
                                } else {
                                    debug!(
                                        "No active session for task {} - setting status to idle",
                                        task.id
                                    );

                                    let mut handled = false;
                                    if let Some(session_name) = task.tmux_session_name.as_deref() {
                                        let _ = db.update_task_status(task.id, Status::Dead.as_str());
                                        let _ = db.update_task_status_metadata(
                                            task.id,
                                            SessionStatusSource::None.as_str(),
                                            Some(to_iso8601(fetched_at)),
                                            Some(format!("SESSION_NOT_FOUND: {}", session_name)),
                                        );
                                        handled = true;
                                    }

                                    if !handled {
                                        let _ = db.update_task_status(task.id, Status::Idle.as_str());
                                        let _ = db.update_task_status_metadata(
                                            task.id,
                                            SessionStatusSource::Server.as_str(),
                                            Some(to_iso8601(fetched_at)),
                                            None,
                                        );
                                    }
                                }
                            }
                            Err(err) => {
                                tracing::warn!(
                                    "Failed to fetch status for task {} - skipping status update: {:?}",
                                    task.id, err
                                );

                                if let Some(session_name) = task.tmux_session_name.as_deref() {
                                    let exists = tmux_session_exists(session_name);
                                    let status = if exists {
                                        Status::Running
                                    } else {
                                        Status::Dead
                                    };

                                    let _ = db.update_task_status(task.id, status.as_str());
                                    let _ = db.update_task_status_metadata(
                                        task.id,
                                        SessionStatusSource::None.as_str(),
                                        Some(to_iso8601(fetched_at)),
                                        Some(format!("SERVER_{}", err.code)),
                                    );
                                }
                            }
                        }
                    }

                    interruptible_sleep(staggered_poll_delay(index), &stop).await;
                }
            }
        });
    })
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
async fn interruptible_sleep(duration: Duration, stop: &AtomicBool) {
    let chunk = Duration::from_millis(100);
    let mut remaining = duration;
    while remaining > Duration::ZERO && !stop.load(Ordering::Relaxed) {
        let sleep_duration = remaining.min(chunk);
        tokio::time::sleep(sleep_duration).await;
        remaining = remaining.saturating_sub(sleep_duration);
    }
}
