use anyhow::{Context, Result};
use tuirealm::Update;
use uuid::Uuid;

use crate::db::Database;
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
        } else {
            self.focused_category = 0;
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
            Some(Msg::FocusColumn(index)) => {
                if index < self.categories.len() {
                    self.focused_category = index;
                }
                None
            }
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
