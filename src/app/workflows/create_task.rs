use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use nucleo::{Config, Matcher, Utf32Str};
use tracing::warn;
use uuid::Uuid;

use crate::app::runtime::{
    CreateTaskRuntime, next_available_session_name_by, worktrees_root_for_repo,
};
use crate::app::state::{CreateTaskOutcome, NewTaskDialogState};
use crate::db::Database;
use crate::git::derive_worktree_path;
use crate::matching::{
    ascii_case_insensitive_subsequence, normalize_fuzzy_needle, recency_frequency_bonus,
    safe_fuzzy_indices,
};
use crate::opencode::{Status, opencode_attach_command};
use crate::types::{CommandFrequency, Repo};

const REPO_SELECTION_USAGE_PREFIX: &str = "repo-selection:";
const GENERATED_BRANCH_PREFIX: &str = "feature";

const BRANCH_ADJECTIVES: &[&str] = &[
    "amber", "brisk", "calm", "daring", "eager", "frost", "golden", "honest", "ivory", "jolly",
    "kind", "lunar", "mellow", "nimble", "opal", "proud", "quiet", "rapid", "solar", "tidy",
    "urban", "vivid", "wise", "young", "zesty",
];

const BRANCH_NOUNS: &[&str] = &[
    "badger", "beacon", "cedar", "drift", "ember", "falcon", "garden", "harbor", "island",
    "jungle", "kernel", "lagoon", "meadow", "nebula", "otter", "prairie", "quartz", "rocket",
    "summit", "thunder", "uplink", "voyage", "willow", "yonder", "zephyr",
];

pub(crate) fn create_task_pipeline_with_runtime(
    db: &Database,
    repos: &mut Vec<Repo>,
    todo_category_id: Uuid,
    state: &NewTaskDialogState,
    project_slug: Option<&str>,
    runtime: &impl CreateTaskRuntime,
) -> Result<CreateTaskOutcome> {
    let mut warning = None;
    let (repo, branch, repo_path, worktree_path, remove_worktree_on_failure) = if state
        .use_existing_directory
    {
        let existing_dir_input = state.existing_dir_input.trim();
        if existing_dir_input.is_empty() {
            anyhow::bail!("existing directory cannot be empty");
        }

        let existing_dir_path = PathBuf::from(existing_dir_input);
        if !existing_dir_path.exists() {
            anyhow::bail!(
                "existing directory does not exist: {}",
                existing_dir_path.display()
            );
        }
        if !existing_dir_path.is_dir() {
            anyhow::bail!(
                "existing directory is not a folder: {}",
                existing_dir_path.display()
            );
        }
        if !runtime.git_is_valid_repo(&existing_dir_path) {
            anyhow::bail!(
                "existing directory is not a git repository: {}",
                existing_dir_path.display()
            );
        }

        let canonical = fs::canonicalize(&existing_dir_path).with_context(|| {
            format!(
                "failed to canonicalize existing directory {}",
                existing_dir_path.display()
            )
        })?;

        let repo_root = runtime
            .git_resolve_repo_root(&canonical)
            .context("failed to resolve repository root for existing directory")?;
        let canonical_repo_root = fs::canonicalize(&repo_root).with_context(|| {
            format!(
                "failed to canonicalize repository root {}",
                repo_root.display()
            )
        })?;
        let repo = resolve_repo_for_existing_directory(db, repos, &canonical_repo_root)?;

        let branch = runtime
            .git_current_branch(&canonical)
            .context("failed to detect branch from existing directory")?;
        if branch.trim().is_empty() {
            anyhow::bail!("existing directory is in detached HEAD state; switch to a branch first");
        }

        (
            repo,
            branch.trim().to_string(),
            canonical_repo_root,
            canonical,
            false,
        )
    } else {
        let repo = resolve_repo_for_creation(db, repos, state, runtime)?;
        let repo_path = PathBuf::from(&repo.path);

        let branch =
            resolve_create_task_branch(state.branch_input.trim(), state.title_input.trim())?;
        runtime
            .git_validate_branch(&repo_path, &branch)
            .context("branch validation failed")?;

        let base_ref = if state.base_input.trim().is_empty() {
            runtime.git_detect_default_branch(&repo_path)
        } else {
            state.base_input.trim().to_string()
        };

        if let Err(err) = runtime.git_fetch(&repo_path) {
            let message = format!("fetch from origin failed, continuing offline: {err:#}");
            tracing::warn!("{message}");
            warning = Some(message);
        }

        if state.ensure_base_up_to_date {
            runtime
                .git_check_branch_up_to_date(&repo_path, &base_ref)
                .context("base branch check failed")?;
        }

        let worktrees_root = worktrees_root_for_repo(&repo_path);
        fs::create_dir_all(&worktrees_root).with_context(|| {
            format!(
                "failed to create worktree root {}",
                worktrees_root.display()
            )
        })?;
        let derived_worktree_path = derive_worktree_path(&worktrees_root, &repo_path, &branch);

        runtime
            .git_create_worktree(&repo_path, &derived_worktree_path, &branch, &base_ref)
            .context("worktree creation failed")?;

        (repo, branch, repo_path, derived_worktree_path, true)
    };

    let mut created_session_name: Option<String> = None;
    let mut created_task_id: Option<Uuid> = None;
    let branch_name = branch.clone();
    let resolved_title = resolve_task_title(state.title_input.trim(), &branch_name);

    let mut operation = || -> Result<()> {
        let session_name =
            next_available_session_name_by(None, project_slug, &repo.name, &branch_name, |name| {
                runtime.tmux_session_exists(name)
            });

        let command = opencode_attach_command(None, Some(worktree_path.to_string_lossy().as_ref()));

        runtime
            .tmux_create_session(&session_name, &worktree_path, Some(&command))
            .context("tmux session creation failed")?;
        created_session_name = Some(session_name.clone());

        let task = db
            .add_task(repo.id, &branch_name, &resolved_title, todo_category_id)
            .context("failed to save task")?;
        created_task_id = Some(task.id);

        db.update_task_tmux(
            task.id,
            Some(session_name.clone()),
            Some(worktree_path.display().to_string()),
        )
        .context("failed to save task runtime metadata")?;
        db.update_task_status(task.id, Status::Idle.as_str())
            .context("failed to save task runtime status")?;

        if let Err(err) = db.increment_command_usage(&repo_selection_command_id(repo.id)) {
            warn!(
                error = %err,
                repo_id = %repo.id,
                "failed to persist repo selection usage"
            );
        }

        Ok(())
    };

    if let Err(err) = operation() {
        if let Some(task_id) = created_task_id {
            let _ = db.delete_task(task_id);
        }
        if let Some(session_name) = created_session_name {
            let _ = runtime.tmux_kill_session(&session_name);
        }
        if remove_worktree_on_failure {
            let _ = runtime.git_remove_worktree(&repo_path, &worktree_path);
        }
        return Err(err);
    }

    Ok(CreateTaskOutcome { warning })
}

fn resolve_create_task_branch(branch_input: &str, title_input: &str) -> Result<String> {
    let branch = branch_input.trim();
    let title = title_input.trim();
    if branch.is_empty() && title.is_empty() {
        anyhow::bail!("enter branch or title");
    }
    if branch.is_empty() {
        return Ok(generate_human_readable_branch_slug());
    }
    Ok(branch.to_string())
}

fn generate_human_readable_branch_slug() -> String {
    let bytes = Uuid::new_v4().into_bytes();
    let adjective = BRANCH_ADJECTIVES[bytes[0] as usize % BRANCH_ADJECTIVES.len()];
    let noun = BRANCH_NOUNS[bytes[1] as usize % BRANCH_NOUNS.len()];
    let suffix = u16::from_be_bytes([bytes[2], bytes[3]]) % 1000;
    format!("{GENERATED_BRANCH_PREFIX}/{adjective}-{noun}-{suffix:03}")
}

fn resolve_task_title(title_input: &str, branch_name: &str) -> String {
    let title = title_input.trim();
    if title.is_empty() {
        return branch_name.to_string();
    }
    title.to_string()
}

pub(crate) fn resolve_repo_for_creation(
    db: &Database,
    repos: &mut Vec<Repo>,
    state: &NewTaskDialogState,
    runtime: &impl CreateTaskRuntime,
) -> Result<Repo> {
    let repo_path_input = state.repo_input.trim();
    if !repo_path_input.is_empty() {
        let path = PathBuf::from(repo_path_input);
        let path_exists = path.exists();
        if path_exists && runtime.git_is_valid_repo(&path) {
            let canonical = fs::canonicalize(&path)
                .with_context(|| format!("failed to canonicalize repo path {}", path.display()))?;
            if let Some(existing) = repos
                .iter()
                .find(|repo| Path::new(&repo.path) == canonical)
                .cloned()
            {
                return Ok(existing);
            }

            let repo = db
                .add_repo(&canonical)
                .with_context(|| format!("failed to save repo {}", canonical.display()))?;
            repos.push(repo.clone());
            return Ok(repo);
        }

        let usage = repo_selection_usage_map(db);
        if let Some(repo_idx) = rank_repos_for_query(repo_path_input, repos, &usage)
            .first()
            .copied()
        {
            return Ok(repos[repo_idx].clone());
        }

        if path_exists {
            anyhow::bail!("not a git repository: {}", path.display());
        }

        anyhow::bail!("repo path does not exist: {}", path.display());
    }

    repos
        .get(state.repo_idx)
        .cloned()
        .context("select a repo or enter a repository path")
}

fn resolve_repo_for_existing_directory(
    db: &Database,
    repos: &mut Vec<Repo>,
    repo_root: &Path,
) -> Result<Repo> {
    if let Some(existing) = repos
        .iter()
        .find(|repo| Path::new(&repo.path) == repo_root)
        .cloned()
    {
        return Ok(existing);
    }

    let repo = db
        .add_repo(repo_root)
        .with_context(|| format!("failed to save repo {}", repo_root.display()))?;
    repos.push(repo.clone());
    Ok(repo)
}

pub(crate) fn repo_selection_command_id(repo_id: Uuid) -> String {
    format!("{REPO_SELECTION_USAGE_PREFIX}{repo_id}")
}

pub(crate) fn repo_selection_usage_map(db: &Database) -> HashMap<Uuid, CommandFrequency> {
    db.get_command_frequencies()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|(command_id, frequency)| {
            let raw_repo_id = command_id.strip_prefix(REPO_SELECTION_USAGE_PREFIX)?;
            let repo_id = Uuid::parse_str(raw_repo_id).ok()?;
            Some((repo_id, frequency))
        })
        .collect()
}

pub(crate) fn rank_repos_for_query(
    query: &str,
    repos: &[Repo],
    usage: &HashMap<Uuid, CommandFrequency>,
) -> Vec<usize> {
    if repos.is_empty() {
        return Vec::new();
    }

    let now = Utc::now();
    let normalized_query = normalize_fuzzy_needle(query);
    let mut ranked: Vec<(usize, f64)> = Vec::with_capacity(repos.len());

    if normalized_query.is_empty() {
        for (repo_idx, repo) in repos.iter().enumerate() {
            ranked.push((repo_idx, repo_selection_bonus(repo.id, usage, now)));
        }
    } else {
        let mut matcher = Matcher::new(Config::DEFAULT);
        let mut query_buf = Vec::new();
        let query_utf32 = Utf32Str::new(normalized_query.as_str(), &mut query_buf);
        let mut candidate_buf = Vec::new();
        let mut matched_indices = Vec::new();

        for (repo_idx, repo) in repos.iter().enumerate() {
            let mut best_match_score: Option<f64> = None;

            for (candidate, candidate_bonus) in repo_match_candidates(repo) {
                matched_indices.clear();
                if !ascii_case_insensitive_subsequence(
                    candidate.as_str(),
                    normalized_query.as_str(),
                ) {
                    continue;
                }
                let candidate_utf32 = Utf32Str::new(candidate.as_str(), &mut candidate_buf);
                if let Some(fuzzy_score) = safe_fuzzy_indices(
                    &mut matcher,
                    candidate_utf32,
                    query_utf32,
                    &mut matched_indices,
                ) {
                    let score = f64::from(fuzzy_score) + candidate_bonus;
                    best_match_score = Some(match best_match_score {
                        Some(current) => current.max(score),
                        None => score,
                    });
                }
            }

            if let Some(best_match_score) = best_match_score {
                let score = best_match_score + repo_selection_bonus(repo.id, usage, now);
                ranked.push((repo_idx, score));
            }
        }
    }

    ranked.sort_by(|left, right| {
        right
            .1
            .partial_cmp(&left.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.0.cmp(&right.0))
    });

    ranked.into_iter().map(|(repo_idx, _)| repo_idx).collect()
}

pub(crate) fn repo_match_candidates(repo: &Repo) -> Vec<(String, f64)> {
    let mut out: Vec<(String, f64)> = Vec::new();
    let mut seen = HashSet::new();
    let mut add = |value: String, bonus: f64| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return;
        }
        let normalized = trimmed.to_ascii_lowercase();
        if seen.insert(normalized) {
            out.push((trimmed.to_string(), bonus));
        }
    };

    add(repo.name.clone(), 90.0);
    add(repo.path.clone(), 65.0);

    let path = Path::new(&repo.path);
    if let Some(file_name) = path.file_name().and_then(|value| value.to_str()) {
        add(file_name.to_string(), 85.0);
    }

    let segments: Vec<String> = path
        .components()
        .filter_map(|component| match component {
            std::path::Component::Normal(segment) => Some(segment.to_string_lossy().to_string()),
            _ => None,
        })
        .filter(|segment| !segment.is_empty())
        .collect();

    for segment in &segments {
        add(segment.to_string(), 80.0);
    }

    if segments.len() >= 2 {
        let suffix = format!(
            "{}/{}",
            segments[segments.len() - 2],
            segments[segments.len() - 1]
        );
        add(suffix, 88.0);
    }

    if segments.len() >= 3 {
        let suffix = format!(
            "{}/{}/{}",
            segments[segments.len() - 3],
            segments[segments.len() - 2],
            segments[segments.len() - 1]
        );
        add(suffix, 92.0);
    }

    out
}

fn repo_selection_bonus(
    repo_id: Uuid,
    usage: &HashMap<Uuid, CommandFrequency>,
    now: DateTime<Utc>,
) -> f64 {
    let Some(freq) = usage.get(&repo_id) else {
        return 0.0;
    };

    recency_frequency_bonus(
        freq.use_count,
        &freq.last_used,
        now,
        0.35,
        0.65,
        48.0,
        120.0,
    )
}

#[cfg(test)]
mod tests {
    use super::{
        generate_human_readable_branch_slug, rank_repos_for_query, resolve_create_task_branch,
        resolve_task_title,
    };
    use crate::types::Repo;
    use std::collections::HashMap;
    use uuid::Uuid;

    #[test]
    fn resolve_create_task_branch_rejects_empty_branch_and_title() {
        let err = resolve_create_task_branch("", "").expect_err("empty branch+title must fail");
        assert!(err.to_string().contains("enter branch or title"));
    }

    #[test]
    fn resolve_create_task_branch_uses_input_branch_when_present() {
        let branch = resolve_create_task_branch("feature/manual", "").expect("branch should pass");
        assert_eq!(branch, "feature/manual");
    }

    #[test]
    fn resolve_create_task_branch_generates_human_slug_when_branch_is_empty() {
        let branch =
            resolve_create_task_branch("", "Ship session flow").expect("branch should generate");
        assert!(branch.starts_with("feature/"));
        assert!(branch.len() > "feature/a-b-000".len());
    }

    #[test]
    fn generated_branch_slug_has_expected_shape() {
        let branch = generate_human_readable_branch_slug();
        assert!(branch.starts_with("feature/"));
        let slug = branch.trim_start_matches("feature/");
        let parts: Vec<&str> = slug.split('-').collect();
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[2].len(), 3);
        assert!(parts[2].chars().all(|ch| ch.is_ascii_digit()));
    }

    #[test]
    fn resolve_task_title_uses_branch_when_empty() {
        let title = resolve_task_title("", "feature/amber-otter-001");
        assert_eq!(title, "feature/amber-otter-001");
    }

    #[test]
    fn resolve_task_title_preserves_non_empty_title() {
        let title = resolve_task_title("Ship session flow", "feature/amber-otter-001");
        assert_eq!(title, "Ship session flow");
    }

    #[test]
    fn rank_repos_for_query_normalizes_query_before_matching() {
        let repo = Repo {
            id: Uuid::new_v4(),
            path: "/tmp/opencode-kanban/alpha-repo".to_string(),
            name: "Alpha Repo".to_string(),
            default_base: Some("main".to_string()),
            remote_url: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        };

        let ranked = rank_repos_for_query("A\nL\tP", &[repo], &HashMap::new());
        assert_eq!(ranked, vec![0]);
    }
}
