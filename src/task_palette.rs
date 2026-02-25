use crate::app::Message;
use crate::matching::{
    ascii_case_insensitive_subsequence, normalize_fuzzy_needle, safe_fuzzy_indices,
};
use nucleo::{Config, Matcher, Utf32Str};
use std::collections::HashSet;
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct TaskPaletteCandidate {
    pub project_name: String,
    pub project_path: PathBuf,
    pub task_id: Uuid,
    pub title: String,
    pub branch: String,
    pub repo_name: String,
    pub category_name: String,
}

impl TaskPaletteCandidate {
    pub fn display_title(&self) -> String {
        self.title.clone()
    }

    pub fn display_context(&self) -> String {
        format!(
            "{}  ·  {}  ·  {}  ·  {}",
            self.project_name, self.category_name, self.repo_name, self.branch
        )
    }

    fn search_label(&self) -> String {
        format!(
            "{} {} {} {} {}",
            self.title, self.branch, self.repo_name, self.category_name, self.project_name
        )
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RankedTaskCandidate {
    pub candidate_idx: usize,
    pub score: f64,
    pub matched_indices: Vec<usize>,
    pub match_parts: TaskPaletteMatchParts,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TaskPaletteMatchParts {
    pub title: Vec<usize>,
    pub branch: Vec<usize>,
    pub repo_name: Vec<usize>,
    pub category_name: Vec<usize>,
    pub project_name: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TaskPaletteState {
    pub query: String,
    pub scope_label: String,
    pub selected_index: usize,
    pub filtered: Vec<RankedTaskCandidate>,
    pub candidates: Vec<TaskPaletteCandidate>,
}

impl TaskPaletteState {
    pub fn new(candidates: Vec<TaskPaletteCandidate>) -> Self {
        Self::new_with_scope(candidates, "Global".to_string())
    }

    pub fn new_with_scope(candidates: Vec<TaskPaletteCandidate>, scope_label: String) -> Self {
        let mut state = Self {
            query: String::new(),
            scope_label,
            selected_index: 0,
            filtered: Vec::new(),
            candidates,
        };
        state.update_query();
        state
    }

    pub fn update_query(&mut self) {
        let previous_len = self.filtered.len();
        self.filtered = rank_task_candidates(&self.query, &self.candidates);
        if self.filtered.is_empty() || self.filtered.len() < previous_len {
            self.selected_index = 0;
        } else {
            self.selected_index = self.selected_index.min(self.filtered.len() - 1);
        }
    }

    pub fn move_selection(&mut self, delta: isize) {
        if self.filtered.is_empty() {
            self.selected_index = 0;
            return;
        }

        let len = self.filtered.len() as isize;
        let next = (self.selected_index as isize + delta).rem_euclid(len);
        self.selected_index = next as usize;
    }

    pub fn selected_jump_message(&self) -> Option<Message> {
        let selected = self.filtered.get(self.selected_index)?;
        let target = self.candidates.get(selected.candidate_idx)?;
        Some(Message::JumpToTaskFromPalette(
            target.project_path.clone(),
            target.task_id,
        ))
    }

    pub fn candidate_for_ranked(
        &self,
        ranked: &RankedTaskCandidate,
    ) -> Option<&TaskPaletteCandidate> {
        self.candidates.get(ranked.candidate_idx)
    }

    pub fn selected_position(&self) -> Option<usize> {
        if self.filtered.is_empty() {
            None
        } else {
            Some(self.selected_index.min(self.filtered.len() - 1) + 1)
        }
    }
}

pub fn rank_task_candidates(
    query: &str,
    candidates: &[TaskPaletteCandidate],
) -> Vec<RankedTaskCandidate> {
    let normalized_query = normalize_fuzzy_needle(query);
    let mut ranked = Vec::with_capacity(candidates.len());

    if normalized_query.is_empty() {
        for (idx, _candidate) in candidates.iter().enumerate() {
            ranked.push(RankedTaskCandidate {
                candidate_idx: idx,
                score: 0.0,
                matched_indices: Vec::new(),
                match_parts: TaskPaletteMatchParts::default(),
            });
        }

        ranked.sort_by(|a, b| {
            let left = &candidates[a.candidate_idx];
            let right = &candidates[b.candidate_idx];
            left.project_name
                .cmp(&right.project_name)
                .then_with(|| left.category_name.cmp(&right.category_name))
                .then_with(|| left.title.cmp(&right.title))
                .then_with(|| left.branch.cmp(&right.branch))
        });
        return ranked;
    }

    let mut matcher = Matcher::new(Config::DEFAULT);
    let mut query_buf = Vec::new();
    let query_utf32 = Utf32Str::new(normalized_query.as_str(), &mut query_buf);
    let mut label_buf = Vec::new();
    let mut matched_indices = Vec::new();

    for (idx, candidate) in candidates.iter().enumerate() {
        let label = candidate.search_label();
        matched_indices.clear();
        if !ascii_case_insensitive_subsequence(&label, normalized_query.as_str()) {
            continue;
        }

        let label_utf32 = Utf32Str::new(label.as_str(), &mut label_buf);
        if let Some(score) =
            safe_fuzzy_indices(&mut matcher, label_utf32, query_utf32, &mut matched_indices)
        {
            ranked.push(RankedTaskCandidate {
                candidate_idx: idx,
                score: f64::from(score),
                matched_indices: matched_indices
                    .iter()
                    .map(|index| *index as usize)
                    .collect(),
                match_parts: map_match_parts(candidate, &matched_indices),
            });
        }
    }

    ranked.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.candidate_idx.cmp(&b.candidate_idx))
    });

    ranked
}

fn map_match_parts(
    candidate: &TaskPaletteCandidate,
    matched_indices: &[u32],
) -> TaskPaletteMatchParts {
    let title_len = candidate.title.chars().count();
    let branch_len = candidate.branch.chars().count();
    let repo_len = candidate.repo_name.chars().count();
    let category_len = candidate.category_name.chars().count();
    let project_len = candidate.project_name.chars().count();

    let title_start = 0usize;
    let branch_start = title_start + title_len + 1;
    let repo_start = branch_start + branch_len + 1;
    let category_start = repo_start + repo_len + 1;
    let project_start = category_start + category_len + 1;
    let project_end = project_start + project_len;

    let mut parts = TaskPaletteMatchParts::default();
    for &index in matched_indices {
        let index = index as usize;
        if index < branch_start.saturating_sub(1) {
            parts.title.push(index.saturating_sub(title_start));
        } else if index >= branch_start && index < repo_start.saturating_sub(1) {
            parts.branch.push(index - branch_start);
        } else if index >= repo_start && index < category_start.saturating_sub(1) {
            parts.repo_name.push(index - repo_start);
        } else if index >= category_start && index < project_start.saturating_sub(1) {
            parts.category_name.push(index - category_start);
        } else if index >= project_start && index < project_end {
            parts.project_name.push(index - project_start);
        }
    }

    dedup_match_indices(&mut parts.title);
    dedup_match_indices(&mut parts.branch);
    dedup_match_indices(&mut parts.repo_name);
    dedup_match_indices(&mut parts.category_name);
    dedup_match_indices(&mut parts.project_name);

    parts
}

fn dedup_match_indices(indices: &mut Vec<usize>) {
    if indices.len() < 2 {
        return;
    }
    let mut seen = HashSet::with_capacity(indices.len());
    indices.retain(|index| seen.insert(*index));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(
        project: &str,
        title: &str,
        branch: &str,
        repo: &str,
        category: &str,
    ) -> TaskPaletteCandidate {
        TaskPaletteCandidate {
            project_name: project.to_string(),
            project_path: PathBuf::from(format!("/tmp/{project}.sqlite")),
            task_id: Uuid::new_v4(),
            title: title.to_string(),
            branch: branch.to_string(),
            repo_name: repo.to_string(),
            category_name: category.to_string(),
        }
    }

    #[test]
    fn empty_query_returns_all_candidates() {
        let items = vec![
            candidate("beta", "Task B", "feat/b", "repo-b", "In Progress"),
            candidate("alpha", "Task A", "feat/a", "repo-a", "Todo"),
        ];

        let ranked = rank_task_candidates("", &items);
        assert_eq!(ranked.len(), 2);
    }

    #[test]
    fn fuzzy_query_filters_candidates() {
        let items = vec![
            candidate("alpha", "Fix login flow", "feat/login", "web", "Todo"),
            candidate("alpha", "Improve docs", "docs/update", "docs", "Done"),
        ];

        let ranked = rank_task_candidates("lgfl", &items);
        assert_eq!(ranked.len(), 1);
        assert_eq!(items[ranked[0].candidate_idx].title, "Fix login flow");
    }

    #[test]
    fn selected_jump_message_uses_selected_candidate() {
        let items = vec![candidate(
            "alpha",
            "Fix login flow",
            "feat/login",
            "web",
            "Todo",
        )];
        let state = TaskPaletteState::new(items.clone());
        let message = state.selected_jump_message();
        assert!(matches!(
            message,
            Some(Message::JumpToTaskFromPalette(_, _))
        ));
    }

    #[test]
    fn fuzzy_query_maps_matches_per_field() {
        let items = vec![candidate(
            "tea",
            "Improve task search",
            "feature/task-search",
            "opencode-kanban",
            "IN PROGRESS",
        )];

        let ranked = rank_task_candidates("tsk", &items);
        assert_eq!(ranked.len(), 1);
        assert!(!ranked[0].match_parts.title.is_empty());
    }
}
