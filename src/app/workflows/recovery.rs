use std::collections::HashMap;
use std::path::Path;

use anyhow::Result;
use uuid::Uuid;

use crate::app::runtime::RecoveryRuntime;
use crate::app::state::{DesiredTaskState, ObservedTaskState};
use crate::db::Database;
use crate::opencode::Status;
use crate::types::{Repo, SessionState, Task};

fn desired_state_for_task(task: &Task, repo_available: bool) -> DesiredTaskState {
    DesiredTaskState {
        expected_session_name: task.tmux_session_name.clone(),
        repo_available,
    }
}

fn observed_state_for_task(
    desired: &DesiredTaskState,
    runtime: &impl RecoveryRuntime,
) -> ObservedTaskState {
    if !desired.repo_available {
        return ObservedTaskState {
            repo_available: false,
            session_exists: false,
            session_status: None,
        };
    }

    let Some(session_name) = desired.expected_session_name.as_deref() else {
        return ObservedTaskState {
            repo_available: true,
            session_exists: false,
            session_status: None,
        };
    };

    if !runtime.session_exists(session_name) {
        return ObservedTaskState {
            repo_available: true,
            session_exists: false,
            session_status: None,
        };
    }

    ObservedTaskState {
        repo_available: true,
        session_exists: true,
        session_status: None,
    }
}

fn reconcile_desired_vs_observed(
    desired: &DesiredTaskState,
    observed: &ObservedTaskState,
    current_status: &str,
) -> String {
    if !desired.repo_available || !observed.repo_available {
        return Status::Idle.as_str().to_string();
    }

    if desired.expected_session_name.is_none() || !observed.session_exists {
        return Status::Idle.as_str().to_string();
    }

    observed
        .session_status
        .as_ref()
        .map(|status| status.state.as_str().to_string())
        .unwrap_or_else(|| {
            if SessionState::from_raw_status(current_status) == SessionState::Running {
                Status::Running.as_str().to_string()
            } else {
                Status::Idle.as_str().to_string()
            }
        })
}

pub(crate) fn reconcile_startup_tasks(
    db: &Database,
    tasks: &[Task],
    repos: &[Repo],
    runtime: &impl RecoveryRuntime,
) -> Result<()> {
    let repos_by_id: HashMap<Uuid, &Repo> = repos.iter().map(|repo| (repo.id, repo)).collect();

    for task in tasks {
        let repo_available = repos_by_id
            .get(&task.repo_id)
            .map(|repo| runtime.repo_exists(Path::new(&repo.path)))
            .unwrap_or(false);

        let desired = desired_state_for_task(task, repo_available);
        let observed = observed_state_for_task(&desired, runtime);
        let reconciled_status =
            reconcile_desired_vs_observed(&desired, &observed, &task.tmux_status);

        if reconciled_status != task.tmux_status {
            tracing::debug!(
                task_id = %task.id,
                previous = %task.tmux_status,
                reconciled = %reconciled_status,
                "startup recovery reconciliation updated task status"
            );
            db.update_task_status(task.id, &reconciled_status)?;
        }
    }

    Ok(())
}
