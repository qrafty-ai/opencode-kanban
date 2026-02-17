use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result};
use tuirealm::Update;
use uuid::Uuid;

use crate::app::runtime::{RealRecoveryRuntime, RecoveryRuntime, next_available_session_name};
use crate::app::state::STATUS_REPO_UNAVAILABLE;
use crate::db::Database;
use crate::opencode::{Status, opencode_attach_command};
use crate::types::{Category, Repo, Task};

use super::messages::Msg;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct TaskCreationRequest {
    pub repo_id: Uuid,
    pub branch: String,
    pub title: String,
    pub category_id: Option<Uuid>,
}

pub struct Model {
    pub db: Database,
    pub tasks: Vec<Task>,
    pub categories: Vec<Category>,
    pub repos: Vec<Repo>,
    pub focused_category: usize,
    pub selected_task_per_column: HashMap<usize, usize>,
    pub pending_task_creation: Option<TaskCreationRequest>,
    pub pending_task_deletion: Option<Uuid>,
    pub last_error: Option<String>,
}

impl Model {
    pub fn new(db: Database) -> Result<Self> {
        let mut model = Self {
            db,
            tasks: Vec::new(),
            categories: Vec::new(),
            repos: Vec::new(),
            focused_category: 0,
            selected_task_per_column: HashMap::new(),
            pending_task_creation: None,
            pending_task_deletion: None,
            last_error: None,
        };
        model.refresh_data()?;
        Ok(model)
    }

    pub fn refresh_data(&mut self) -> Result<()> {
        self.tasks = self.db.list_tasks().context("failed to load tasks")?;
        self.categories = self
            .db
            .list_categories()
            .context("failed to load categories")?;
        self.repos = self.db.list_repos().context("failed to load repos")?;

        if !self.categories.is_empty() {
            self.focused_category = self.focused_category.min(self.categories.len() - 1);
            self.selected_task_per_column
                .retain(|column, _| *column < self.categories.len());
            self.selected_task_per_column
                .entry(self.focused_category)
                .or_insert(0);
        } else {
            self.focused_category = 0;
            self.selected_task_per_column.clear();
        }

        Ok(())
    }

    pub fn queue_task_creation(&mut self, request: TaskCreationRequest) {
        self.pending_task_creation = Some(request);
    }

    pub fn queue_task_deletion(&mut self, task_id: Uuid) {
        self.pending_task_deletion = Some(task_id);
    }

    fn apply_task_creation(&mut self) -> Result<()> {
        let Some(request) = self.pending_task_creation.clone() else {
            return Ok(());
        };

        let category_id = match request.category_id {
            Some(category_id) => category_id,
            None => self.default_category_id()?,
        };

        self.db
            .add_task(request.repo_id, request.branch, request.title, category_id)
            .context("failed to create task")?;
        self.pending_task_creation = None;
        self.refresh_data()?;
        Ok(())
    }

    fn apply_task_deletion(&mut self) -> Result<()> {
        let Some(task_id) = self.pending_task_deletion else {
            return Ok(());
        };

        self.db
            .delete_task(task_id)
            .context("failed to delete task")?;
        self.pending_task_deletion = None;
        self.refresh_data()?;
        Ok(())
    }

    fn default_category_id(&self) -> Result<Uuid> {
        self.categories
            .iter()
            .find(|category| category.name == "TODO")
            .or_else(|| self.categories.first())
            .map(|category| category.id)
            .context("no category available for new task")
    }

    fn tasks_for_column(&self, column_index: usize) -> Vec<Task> {
        let Some(category) = self.categories.get(column_index) else {
            return Vec::new();
        };
        let mut tasks = self
            .tasks
            .iter()
            .filter(|task| task.category_id == category.id)
            .cloned()
            .collect::<Vec<_>>();
        tasks.sort_by(|left, right| {
            left.position
                .cmp(&right.position)
                .then_with(|| left.title.cmp(&right.title))
                .then_with(|| left.id.cmp(&right.id))
        });
        tasks
    }

    fn selected_index_for_column(&self, column_index: usize, task_count: usize) -> Option<usize> {
        if task_count == 0 {
            return None;
        }
        Some(
            self.selected_task_per_column
                .get(&column_index)
                .copied()
                .unwrap_or(0)
                .min(task_count - 1),
        )
    }

    fn selected_task_in_column(&self, column_index: usize) -> Option<Task> {
        let tasks = self.tasks_for_column(column_index);
        let selected = self.selected_index_for_column(column_index, tasks.len())?;
        tasks.get(selected).cloned()
    }

    fn move_task_left(&mut self) -> Result<()> {
        if self.categories.len() < 2 || self.focused_category == 0 {
            return Ok(());
        }

        let source_column = self.focused_category;
        let target_column = source_column - 1;
        let Some(task) = self.selected_task_in_column(source_column) else {
            return Ok(());
        };
        let target_category = self
            .categories
            .get(target_column)
            .context("target category missing for move-left")?;

        self.db
            .update_task_category(task.id, target_category.id, 0)
            .context("failed to move task to previous category")?;

        self.focused_category = target_column;
        self.selected_task_per_column.insert(target_column, 0);
        self.refresh_data()
    }

    fn move_task_right(&mut self) -> Result<()> {
        if self.categories.len() < 2 || self.focused_category + 1 >= self.categories.len() {
            return Ok(());
        }

        let source_column = self.focused_category;
        let target_column = source_column + 1;
        let Some(task) = self.selected_task_in_column(source_column) else {
            return Ok(());
        };
        let target_category = self
            .categories
            .get(target_column)
            .context("target category missing for move-right")?;

        self.db
            .update_task_category(task.id, target_category.id, 0)
            .context("failed to move task to next category")?;

        self.focused_category = target_column;
        self.selected_task_per_column.insert(target_column, 0);
        self.refresh_data()
    }

    fn move_task_up(&mut self) -> Result<()> {
        let column_index = self.focused_category;
        let mut tasks = self.tasks_for_column(column_index);
        if tasks.len() < 2 {
            return Ok(());
        }

        let selected = self
            .selected_index_for_column(column_index, tasks.len())
            .unwrap_or(0);
        if selected == 0 {
            return Ok(());
        }

        tasks.swap(selected - 1, selected);
        for (position, task) in tasks.iter().enumerate() {
            self.db
                .update_task_position(task.id, position as i64)
                .context("failed to update task position while moving up")?;
        }

        self.selected_task_per_column
            .insert(column_index, selected - 1);
        self.refresh_data()
    }

    fn move_task_down(&mut self) -> Result<()> {
        let column_index = self.focused_category;
        let mut tasks = self.tasks_for_column(column_index);
        if tasks.len() < 2 {
            return Ok(());
        }

        let selected = self
            .selected_index_for_column(column_index, tasks.len())
            .unwrap_or(0);
        if selected + 1 >= tasks.len() {
            return Ok(());
        }

        tasks.swap(selected, selected + 1);
        for (position, task) in tasks.iter().enumerate() {
            self.db
                .update_task_position(task.id, position as i64)
                .context("failed to update task position while moving down")?;
        }

        self.selected_task_per_column
            .insert(column_index, selected + 1);
        self.refresh_data()
    }

    fn repo_for_task(&self, task: &Task) -> Option<Repo> {
        self.repos
            .iter()
            .find(|repo| repo.id == task.repo_id)
            .cloned()
    }

    fn attach_selected_task(&mut self) -> Result<()> {
        let Some(task) = self.selected_task_in_column(self.focused_category) else {
            return Ok(());
        };
        let runtime = RealRecoveryRuntime;

        let Some(repo) = self.repo_for_task(&task) else {
            self.db
                .update_task_status(task.id, STATUS_REPO_UNAVAILABLE)
                .context("failed to mark task as repo unavailable")?;
            self.refresh_data()?;
            anyhow::bail!("repository metadata missing for task '{}'", task.title);
        };

        if !runtime.repo_exists(Path::new(&repo.path)) {
            self.db
                .update_task_status(task.id, STATUS_REPO_UNAVAILABLE)
                .context("failed to mark task as repo unavailable")?;
            self.refresh_data()?;
            anyhow::bail!("repository unavailable: {}", repo.path);
        }

        if let Some(session_name) = task.tmux_session_name.as_deref()
            && runtime.session_exists(session_name)
        {
            runtime
                .switch_client(session_name)
                .with_context(|| format!("failed to switch to tmux session '{session_name}'"))?;
            return Ok(());
        }

        let Some(worktree_path_str) = task.worktree_path.as_deref() else {
            anyhow::bail!("worktree missing for task '{}'", task.title);
        };
        let worktree_path = Path::new(worktree_path_str);
        if !runtime.worktree_exists(worktree_path) {
            anyhow::bail!("worktree not found: {}", worktree_path.display());
        }

        let session_name = next_available_session_name(
            task.tmux_session_name.as_deref(),
            None,
            &repo.name,
            &task.branch,
            &runtime,
        );
        let command = opencode_attach_command(None, task.worktree_path.as_deref());
        runtime
            .create_session(&session_name, worktree_path, &command)
            .with_context(|| format!("failed to create tmux session '{session_name}'"))?;

        self.db
            .update_task_tmux(
                task.id,
                Some(session_name.clone()),
                task.worktree_path.clone(),
            )
            .context("failed to persist task tmux metadata")?;
        self.db
            .update_task_status(task.id, Status::Idle.as_str())
            .context("failed to persist task status")?;
        self.refresh_data()?;

        runtime
            .switch_client(&session_name)
            .with_context(|| format!("failed to switch to tmux session '{session_name}'"))?;
        Ok(())
    }

    fn show_error(&mut self, error: anyhow::Error) -> Option<Msg> {
        let detail = error.to_string();
        self.last_error = Some(detail.clone());
        Some(Msg::ShowError(detail))
    }
}

impl Update<Msg> for Model {
    fn update(&mut self, msg: Option<Msg>) -> Option<Msg> {
        match msg {
            Some(Msg::CreateTask) => match self.apply_task_creation() {
                Ok(()) => None,
                Err(error) => self.show_error(error),
            },
            Some(Msg::ConfirmDeleteTask) => match self.apply_task_deletion() {
                Ok(()) => None,
                Err(error) => self.show_error(error),
            },
            Some(Msg::NavigateLeft) => {
                if self.focused_category > 0 {
                    self.focused_category -= 1;
                    self.selected_task_per_column
                        .entry(self.focused_category)
                        .or_insert(0);
                }
                None
            }
            Some(Msg::NavigateRight) => {
                if self.focused_category + 1 < self.categories.len() {
                    self.focused_category += 1;
                    self.selected_task_per_column
                        .entry(self.focused_category)
                        .or_insert(0);
                }
                None
            }
            Some(Msg::SelectUp) => {
                let selected = self
                    .selected_task_per_column
                    .entry(self.focused_category)
                    .or_insert(0);
                *selected = selected.saturating_sub(1);
                None
            }
            Some(Msg::SelectDown) => {
                let max_index = self
                    .tasks_for_column(self.focused_category)
                    .len()
                    .saturating_sub(1);
                let selected = self
                    .selected_task_per_column
                    .entry(self.focused_category)
                    .or_insert(0);
                *selected = (*selected + 1).min(max_index);
                None
            }
            Some(Msg::FocusColumn(index)) => {
                if index < self.categories.len() {
                    self.focused_category = index;
                    self.selected_task_per_column.entry(index).or_insert(0);
                }
                None
            }
            Some(Msg::SelectTask { column, task }) => {
                if column < self.categories.len() {
                    let max_index = self.tasks_for_column(column).len().saturating_sub(1);
                    self.focused_category = column;
                    self.selected_task_per_column
                        .insert(column, task.min(max_index));
                }
                None
            }
            Some(Msg::MoveTaskLeft) => match self.move_task_left() {
                Ok(()) => None,
                Err(error) => self.show_error(error),
            },
            Some(Msg::MoveTaskRight) => match self.move_task_right() {
                Ok(()) => None,
                Err(error) => self.show_error(error),
            },
            Some(Msg::MoveTaskUp) => match self.move_task_up() {
                Ok(()) => None,
                Err(error) => self.show_error(error),
            },
            Some(Msg::MoveTaskDown) => match self.move_task_down() {
                Ok(()) => None,
                Err(error) => self.show_error(error),
            },
            Some(Msg::AttachTask) => match self.attach_selected_task() {
                Ok(()) => None,
                Err(error) => self.show_error(error),
            },
            _ => None,
        }
    }
}

#[cfg(test)]
mod model {
    use std::path::{Path, PathBuf};
    use std::process::Command;

    use anyhow::Result;
    use tuirealm::Update;
    use uuid::Uuid;

    use crate::db::Database;

    use super::{Model, Msg, TaskCreationRequest};

    #[test]
    fn update_task_created() -> Result<()> {
        let (mut model, repo_dir) = model_with_repo("model-create")?;
        let repo_id = model.repos[0].id;

        model.queue_task_creation(TaskCreationRequest {
            repo_id,
            branch: "feature/model-create".to_string(),
            title: "Model create".to_string(),
            category_id: None,
        });

        let result = Update::update(&mut model, Some(Msg::CreateTask));

        assert_eq!(result, None);
        assert_eq!(model.tasks.len(), 1);
        let created = &model.tasks[0];
        assert_eq!(created.branch, "feature/model-create");
        assert_eq!(created.title, "Model create");
        let todo_id = model
            .categories
            .iter()
            .find(|category| category.name == "TODO")
            .expect("default TODO category should exist")
            .id;
        assert_eq!(created.category_id, todo_id);

        let _ = std::fs::remove_dir_all(repo_dir);
        Ok(())
    }

    #[test]
    fn update_task_deleted() -> Result<()> {
        let (mut model, repo_dir) = model_with_repo("model-delete")?;
        let repo_id = model.repos[0].id;
        let todo_id = model.categories[0].id;

        let task = model
            .db
            .add_task(repo_id, "feature/model-delete", "Model delete", todo_id)?;
        model.refresh_data()?;
        assert_eq!(model.tasks.len(), 1);

        model.queue_task_deletion(task.id);

        let result = Update::update(&mut model, Some(Msg::ConfirmDeleteTask));

        assert_eq!(result, None);
        assert!(model.tasks.is_empty());
        assert!(model.db.get_task(task.id).is_err());

        let _ = std::fs::remove_dir_all(repo_dir);
        Ok(())
    }

    #[test]
    fn update_category_focused() -> Result<()> {
        let (mut model, repo_dir) = model_with_repo("model-focus")?;

        assert_eq!(model.focused_category, 0);

        let result = Update::update(&mut model, Some(Msg::FocusColumn(1)));

        assert_eq!(result, None);
        assert_eq!(model.focused_category, 1);

        let result = Update::update(&mut model, Some(Msg::FocusColumn(99)));
        assert_eq!(result, None);
        assert_eq!(model.focused_category, 1);

        let _ = std::fs::remove_dir_all(repo_dir);
        Ok(())
    }

    #[test]
    fn update_task_moved_right() -> Result<()> {
        let (mut model, repo_dir) = model_with_repo("model-move-right")?;
        let repo_id = model.repos[0].id;
        let todo_id = model.categories[0].id;
        let in_progress_id = model.categories[1].id;

        let created =
            model
                .db
                .add_task(repo_id, "feature/model-move-right", "Move right", todo_id)?;
        model.refresh_data()?;

        let _ = Update::update(&mut model, Some(Msg::FocusColumn(0)));
        let _ = Update::update(&mut model, Some(Msg::SelectTask { column: 0, task: 0 }));
        let result = Update::update(&mut model, Some(Msg::MoveTaskRight));

        assert_eq!(result, None);
        let moved = model.db.get_task(created.id)?;
        assert_eq!(moved.category_id, in_progress_id);
        assert_eq!(model.focused_category, 1);

        let _ = std::fs::remove_dir_all(repo_dir);
        Ok(())
    }

    #[test]
    fn update_task_reordered_down() -> Result<()> {
        let (mut model, repo_dir) = model_with_repo("model-move-down")?;
        let repo_id = model.repos[0].id;
        let todo_id = model.categories[0].id;

        let first = model
            .db
            .add_task(repo_id, "feature/model-down-1", "First", todo_id)?;
        let second = model
            .db
            .add_task(repo_id, "feature/model-down-2", "Second", todo_id)?;
        model.refresh_data()?;

        let _ = Update::update(&mut model, Some(Msg::FocusColumn(0)));
        let _ = Update::update(&mut model, Some(Msg::SelectTask { column: 0, task: 0 }));
        let result = Update::update(&mut model, Some(Msg::MoveTaskDown));

        assert_eq!(result, None);
        let tasks = model.tasks_for_column(0);
        assert_eq!(tasks.len(), 2);
        assert_eq!(tasks[0].id, second.id);
        assert_eq!(tasks[1].id, first.id);

        let _ = std::fs::remove_dir_all(repo_dir);
        Ok(())
    }

    #[test]
    fn update_attach_without_task_is_noop() -> Result<()> {
        let (mut model, repo_dir) = model_with_repo("model-attach-no-task")?;

        let result = Update::update(&mut model, Some(Msg::AttachTask));

        assert_eq!(result, None);
        let _ = std::fs::remove_dir_all(repo_dir);
        Ok(())
    }

    #[test]
    fn update_attach_missing_repo_sets_unavailable_status() -> Result<()> {
        let (mut model, repo_dir) = model_with_repo("model-attach-missing-repo")?;
        let repo_id = model.repos[0].id;
        let todo_id = model.categories[0].id;
        let task = model.db.add_task(
            repo_id,
            "feature/model-attach-missing-repo",
            "Attach",
            todo_id,
        )?;
        model.refresh_data()?;

        std::fs::remove_dir_all(&repo_dir)?;

        let _ = Update::update(&mut model, Some(Msg::SelectTask { column: 0, task: 0 }));
        let result = Update::update(&mut model, Some(Msg::AttachTask));

        let Some(Msg::ShowError(detail)) = result else {
            panic!("attach on missing repo should surface ShowError");
        };
        assert!(
            detail.contains("repository unavailable"),
            "error should include unavailable repo detail"
        );

        let updated = model.db.get_task(task.id)?;
        assert_eq!(updated.tmux_status, "repo_unavailable");

        Ok(())
    }

    fn model_with_repo(name: &str) -> Result<(Model, PathBuf)> {
        let db = Database::open(":memory:")?;
        let repo_dir = create_temp_git_repo(name)?;
        db.add_repo(&repo_dir)?;
        let model = Model::new(db)?;
        Ok((model, repo_dir))
    }

    fn create_temp_git_repo(name: &str) -> Result<PathBuf> {
        let repo_dir =
            std::env::temp_dir().join(format!("opencode-kanban-{name}-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&repo_dir)?;

        run_git_cmd(&repo_dir, &["init", "-b", "main"])?;
        run_git_cmd(
            &repo_dir,
            &[
                "remote",
                "add",
                "origin",
                &format!("https://example.com/{name}.git"),
            ],
        )?;
        run_git_cmd(
            &repo_dir,
            &[
                "symbolic-ref",
                "refs/remotes/origin/HEAD",
                "refs/remotes/origin/main",
            ],
        )?;

        Ok(repo_dir)
    }

    fn run_git_cmd(repo_dir: &Path, args: &[&str]) -> Result<()> {
        let output = Command::new("git")
            .arg("-C")
            .arg(repo_dir)
            .args(args)
            .output()?;
        if !output.status.success() {
            anyhow::bail!(
                "git command failed: git -C {} {}\nstdout: {}\nstderr: {}",
                repo_dir.display(),
                args.join(" "),
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        Ok(())
    }
}
