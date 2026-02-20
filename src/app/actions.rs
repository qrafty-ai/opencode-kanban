//! Task and category action operations

use std::collections::HashMap;
use std::path::Path;

use anyhow::Result;
use uuid::Uuid;

use crate::db::Database;
use crate::git::{git_delete_branch, git_remove_worktree};
use crate::tmux::tmux_kill_session;
use crate::types::{Category, Repo, Task};

use super::state::{ActiveDialog, ErrorDialogState};

/// Move selected task to the left column
pub fn move_task_left(
    db: &Database,
    tasks: &mut [Task],
    categories: &[Category],
    focused_column: &mut usize,
    selected_task_per_column: &mut HashMap<usize, usize>,
) -> Result<()> {
    if *focused_column == 0 {
        return Ok(());
    }
    let Some(category) = categories.get(*focused_column) else {
        return Ok(());
    };
    let task = tasks.iter().find(|t| t.category_id == category.id).cloned();
    let Some(task) = task else {
        return Ok(());
    };
    let target_column = *focused_column - 1;
    let target_category = &categories[target_column];
    db.update_task_category(task.id, target_category.id, 0)?;
    *focused_column = target_column;
    selected_task_per_column.insert(target_column, 0);
    Ok(())
}

/// Move selected task to the right column
pub fn move_task_right(
    db: &Database,
    tasks: &mut [Task],
    categories: &[Category],
    focused_column: &mut usize,
    selected_task_per_column: &mut HashMap<usize, usize>,
) -> Result<()> {
    if *focused_column >= categories.len() - 1 {
        return Ok(());
    }
    let Some(category) = categories.get(*focused_column) else {
        return Ok(());
    };
    let task = tasks.iter().find(|t| t.category_id == category.id).cloned();
    let Some(task) = task else {
        return Ok(());
    };
    let target_column = *focused_column + 1;
    let target_category = &categories[target_column];
    db.update_task_category(task.id, target_category.id, 0)?;
    *focused_column = target_column;
    selected_task_per_column.insert(target_column, 0);
    Ok(())
}

/// Move selected task up in its column
pub fn move_task_up(
    db: &Database,
    tasks: &mut [Task],
    categories: &[Category],
    focused_column: usize,
    selected_task_per_column: &mut HashMap<usize, usize>,
) -> Result<()> {
    let Some(category) = categories.get(focused_column) else {
        return Ok(());
    };
    let mut column_tasks: Vec<_> = tasks
        .iter()
        .filter(|t| t.category_id == category.id)
        .cloned()
        .collect();
    column_tasks.sort_by_key(|t| t.position);
    if column_tasks.len() < 2 {
        return Ok(());
    }
    let selected = selected_task_per_column
        .get(&focused_column)
        .copied()
        .unwrap_or(0)
        .min(column_tasks.len() - 1);
    if selected == 0 {
        return Ok(());
    }
    column_tasks.swap(selected - 1, selected);
    for (idx, task) in column_tasks.iter().enumerate() {
        db.update_task_position(task.id, idx as i64)?;
    }
    selected_task_per_column.insert(focused_column, selected - 1);
    Ok(())
}

/// Move selected task down in its column
pub fn move_task_down(
    db: &Database,
    tasks: &mut [Task],
    categories: &[Category],
    focused_column: usize,
    selected_task_per_column: &mut HashMap<usize, usize>,
) -> Result<()> {
    let Some(category) = categories.get(focused_column) else {
        return Ok(());
    };
    let mut column_tasks: Vec<_> = tasks
        .iter()
        .filter(|t| t.category_id == category.id)
        .cloned()
        .collect();
    column_tasks.sort_by_key(|t| t.position);
    if column_tasks.len() < 2 {
        return Ok(());
    }
    let selected = selected_task_per_column
        .get(&focused_column)
        .copied()
        .unwrap_or(0)
        .min(column_tasks.len() - 1);
    if selected + 1 >= column_tasks.len() {
        return Ok(());
    }
    column_tasks.swap(selected, selected + 1);
    for (idx, task) in column_tasks.iter().enumerate() {
        db.update_task_position(task.id, idx as i64)?;
    }
    selected_task_per_column.insert(focused_column, selected + 1);
    Ok(())
}

/// Confirm category creation or rename
pub fn confirm_category_input(
    db: &Database,
    dialog: &mut ActiveDialog,
    categories: &mut Vec<Category>,
    focused_column: &mut usize,
    selected_task_per_column: &mut HashMap<usize, usize>,
) -> Result<()> {
    let state = match dialog {
        ActiveDialog::CategoryInput(state) => state.clone(),
        _ => return Ok(()),
    };

    let name = state.name_input.trim();
    if name.is_empty() {
        *dialog = ActiveDialog::Error(ErrorDialogState {
            title: "Invalid category".to_string(),
            detail: "Category name cannot be empty.".to_string(),
        });
        return Ok(());
    }

    match state.mode {
        super::state::CategoryInputMode::Add => {
            let next_position = categories
                .iter()
                .map(|category| category.position)
                .max()
                .unwrap_or(-1)
                + 1;
            let created = db.add_category(name, next_position, None)?;
            *dialog = ActiveDialog::None;
            categories.push(created.clone());
            if let Some(index) = categories.iter().position(|c| c.id == created.id) {
                *focused_column = index;
                selected_task_per_column.entry(index).or_insert(0);
            }
        }
        super::state::CategoryInputMode::Rename => {
            let Some(category_id) = state.category_id else {
                return Ok(());
            };
            db.rename_category(category_id, name)?;
            *dialog = ActiveDialog::None;
            if let Some(cat) = categories.iter_mut().find(|c| c.id == category_id) {
                cat.name = name.to_string();
            }
        }
    }

    Ok(())
}

/// Confirm category deletion
pub fn confirm_delete_category(
    db: &Database,
    dialog: &ActiveDialog,
) -> Result<Result<(), ErrorDialogState>> {
    let state = match dialog {
        ActiveDialog::DeleteCategory(state) => state.clone(),
        _ => return Ok(Ok(())),
    };

    if state.task_count > 0 {
        return Ok(Err(ErrorDialogState {
            title: "Category not empty".to_string(),
            detail: format!(
                "Cannot delete '{}' because it still contains {} task(s).",
                state.category_name, state.task_count
            ),
        }));
    }

    db.delete_category(state.category_id)?;
    Ok(Ok(()))
}

/// Confirm task deletion with cleanup
pub fn confirm_delete_task(
    db: &Database,
    tasks: &[Task],
    repos: &[Repo],
    dialog: &ActiveDialog,
) -> Result<Option<Uuid>> {
    let state = match dialog {
        ActiveDialog::DeleteTask(state) => state.clone(),
        _ => return Ok(None),
    };

    let task = tasks.iter().find(|t| t.id == state.task_id);
    let Some(task) = task else {
        return Ok(Some(state.task_id));
    };

    let repo = repos.iter().find(|repo| repo.id == task.repo_id);

    if state.kill_tmux
        && let Some(ref session_name) = task.tmux_session_name
    {
        let _ = tmux_kill_session(session_name);
    }

    if state.remove_worktree
        && let (Some(worktree_path), Some(r)) = (&task.worktree_path, repo.as_ref())
    {
        let worktree = Path::new(worktree_path);
        let repo_path = Path::new(&r.path);
        if worktree.exists() {
            let _ = git_remove_worktree(repo_path, worktree);
        }
    }

    if state.delete_branch
        && let (Some(r), true) = (repo, !task.branch.is_empty())
    {
        let _ = git_delete_branch(Path::new(&r.path), &task.branch);
    }

    db.delete_task(state.task_id)?;
    Ok(Some(state.task_id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{
        CategoryInputDialogState, CategoryInputField, CategoryInputMode, ConfirmCancelField,
        DeleteCategoryDialogState, DeleteTaskDialogState, DeleteTaskField, ErrorDialogState,
    };
    use crate::db::Database;
    use std::path::PathBuf;
    use std::process::Command;

    fn create_temp_git_repo(name: &str) -> PathBuf {
        let repo_dir = std::env::temp_dir().join(format!(
            "opencode-kanban-test-{name}-{}-{}",
            Uuid::new_v4(),
            std::process::id()
        ));
        std::fs::create_dir_all(&repo_dir).unwrap();

        Command::new("git")
            .arg("-C")
            .arg(&repo_dir)
            .args(["init", "-b", "main"])
            .output()
            .expect("git init should work");

        Command::new("git")
            .arg("-C")
            .arg(&repo_dir)
            .args([
                "remote",
                "add",
                "origin",
                &format!("https://example.com/{name}.git"),
            ])
            .output()
            .expect("git remote add should work");

        repo_dir
    }

    fn create_test_db_with_data() -> (Database, Vec<Category>, Vec<Task>, PathBuf) {
        let db = Database::open(":memory:").unwrap();
        let categories = db.list_categories().unwrap();
        let repo_dir = create_temp_git_repo("actions-test");
        let repo = db.add_repo(&repo_dir).unwrap();
        let task1 = db
            .add_task(repo.id, "feature/task1", "Task 1", categories[0].id)
            .unwrap();
        let task2 = db
            .add_task(repo.id, "feature/task2", "Task 2", categories[0].id)
            .unwrap();
        let task3 = db
            .add_task(repo.id, "feature/task3", "Task 3", categories[1].id)
            .unwrap();
        let tasks = vec![task1, task2, task3];
        (db, categories, tasks, repo_dir)
    }

    #[test]
    fn test_move_task_left_at_boundary() {
        let (db, categories, tasks, _repo_dir) = create_test_db_with_data();
        let mut focused_column = 0;
        let mut selected_task_per_column = HashMap::new();
        selected_task_per_column.insert(0, 0);

        // Should do nothing when already at leftmost column
        let result = move_task_left(
            &db,
            &mut tasks.clone(),
            &categories,
            &mut focused_column,
            &mut selected_task_per_column,
        );
        assert!(result.is_ok());
        assert_eq!(focused_column, 0); // Should remain at 0
    }

    #[test]
    fn test_move_task_right_at_boundary() {
        let (db, categories, tasks, _repo_dir) = create_test_db_with_data();
        let last_column = categories.len() - 1;
        let mut focused_column = last_column;
        let mut selected_task_per_column = HashMap::new();
        selected_task_per_column.insert(last_column, 0);

        // Should do nothing when already at rightmost column
        let result = move_task_right(
            &db,
            &mut tasks.clone(),
            &categories,
            &mut focused_column,
            &mut selected_task_per_column,
        );
        assert!(result.is_ok());
        assert_eq!(focused_column, last_column); // Should remain at last column
    }

    #[test]
    fn test_move_task_up_at_boundary() {
        let (db, categories, _tasks, _repo_dir) = create_test_db_with_data();
        let mut tasks = db.list_tasks().unwrap();
        let mut selected_task_per_column = HashMap::new();
        selected_task_per_column.insert(0, 0); // First task selected

        // Should do nothing when already at top
        let result = move_task_up(
            &db,
            &mut tasks,
            &categories,
            0,
            &mut selected_task_per_column,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_move_task_down_at_boundary() {
        let (db, categories, _tasks, _repo_dir) = create_test_db_with_data();
        let mut tasks = db.list_tasks().unwrap();
        let tasks_in_column: Vec<_> = tasks
            .iter()
            .filter(|t| t.category_id == categories[0].id)
            .collect();
        let mut selected_task_per_column = HashMap::new();
        selected_task_per_column.insert(0, tasks_in_column.len() - 1); // Last task selected

        // Should do nothing when already at bottom
        let result = move_task_down(
            &db,
            &mut tasks,
            &categories,
            0,
            &mut selected_task_per_column,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_move_task_left_with_invalid_column() {
        let (db, categories, tasks, _repo_dir) = create_test_db_with_data();
        let mut focused_column = 999; // Invalid column
        let mut selected_task_per_column = HashMap::new();

        let result = move_task_left(
            &db,
            &mut tasks.clone(),
            &categories,
            &mut focused_column,
            &mut selected_task_per_column,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_move_task_right_with_invalid_column() {
        let (db, categories, tasks, _repo_dir) = create_test_db_with_data();
        let mut focused_column = 999; // Invalid column
        let mut selected_task_per_column = HashMap::new();

        let result = move_task_right(
            &db,
            &mut tasks.clone(),
            &categories,
            &mut focused_column,
            &mut selected_task_per_column,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_confirm_category_input_empty_name() {
        let (db, _categories, _tasks, _repo_dir) = create_test_db_with_data();
        let mut dialog = ActiveDialog::CategoryInput(CategoryInputDialogState {
            mode: CategoryInputMode::Add,
            category_id: None,
            name_input: "   ".to_string(), // Empty/whitespace name
            focused_field: CategoryInputField::Name,
        });
        let mut categories = db.list_categories().unwrap();
        let mut focused_column = 0;
        let mut selected_task_per_column = HashMap::new();

        let result = confirm_category_input(
            &db,
            &mut dialog,
            &mut categories,
            &mut focused_column,
            &mut selected_task_per_column,
        );
        assert!(result.is_ok());

        // Dialog should have been changed to Error
        match dialog {
            ActiveDialog::Error(ref error_state) => {
                assert_eq!(error_state.title, "Invalid category");
            }
            _ => panic!("Expected Error dialog, got {:?}", dialog),
        }
    }

    #[test]
    fn test_confirm_category_input_wrong_dialog() {
        let (db, _categories, _tasks, _repo_dir) = create_test_db_with_data();
        let mut dialog = ActiveDialog::None;
        let mut categories = db.list_categories().unwrap();
        let mut focused_column = 0;
        let mut selected_task_per_column = HashMap::new();

        // Should return early when dialog is not CategoryInput
        let result = confirm_category_input(
            &db,
            &mut dialog,
            &mut categories,
            &mut focused_column,
            &mut selected_task_per_column,
        );
        assert!(result.is_ok());
        assert_eq!(dialog, ActiveDialog::None);
    }

    #[test]
    fn test_confirm_delete_category_wrong_dialog() {
        let (db, _categories, _tasks, _repo_dir) = create_test_db_with_data();
        let dialog = ActiveDialog::None;

        let result = confirm_delete_category(&db, &dialog);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Ok(())); // Should return Ok(Ok(())) for wrong dialog
    }

    #[test]
    fn test_confirm_delete_category_with_tasks() {
        let (db, _categories, _tasks, _repo_dir) = create_test_db_with_data();
        let categories = db.list_categories().unwrap();
        let dialog = ActiveDialog::DeleteCategory(DeleteCategoryDialogState {
            category_id: categories[0].id,
            category_name: "TODO".to_string(),
            task_count: 5,
            focused_field: ConfirmCancelField::Confirm,
        });

        let result = confirm_delete_category(&db, &dialog);
        assert!(result.is_ok());

        // Should return Err with error dialog state when category has tasks
        match result.unwrap() {
            Err(ref error_state) => {
                assert_eq!(error_state.title, "Category not empty");
                assert!(error_state.detail.contains("5 task(s)"));
            }
            _ => panic!("Expected Err with ErrorDialogState"),
        }
    }

    #[test]
    fn test_confirm_delete_task_wrong_dialog() {
        let (db, _categories, _tasks, _repo_dir) = create_test_db_with_data();
        let tasks: Vec<Task> = db.list_tasks().unwrap();
        let repos: Vec<Repo> = vec![];
        let dialog = ActiveDialog::None;

        let result = confirm_delete_task(&db, &tasks, &repos, &dialog);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), None); // Should return None for wrong dialog
    }

    #[test]
    fn test_confirm_delete_task_task_not_found() {
        let (db, _categories, _tasks, _repo_dir) = create_test_db_with_data();
        let tasks: Vec<Task> = db.list_tasks().unwrap();
        let repos: Vec<Repo> = vec![];
        let missing_task_id = Uuid::new_v4();
        let dialog = ActiveDialog::DeleteTask(DeleteTaskDialogState {
            task_id: missing_task_id,
            task_title: "Missing".to_string(),
            task_branch: "feature/missing".to_string(),
            kill_tmux: false,
            remove_worktree: false,
            delete_branch: false,
            focused_field: DeleteTaskField::Delete,
        });

        let result = confirm_delete_task(&db, &tasks, &repos, &dialog);
        assert!(result.is_ok());
        // Should return Some(task_id) even when task not found
        assert_eq!(result.unwrap(), Some(missing_task_id));
    }
}
