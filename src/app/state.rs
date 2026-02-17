//! Application state types for dialogs and UI components

use std::str::FromStr;

use uuid::Uuid;

use crate::command_palette::CommandPaletteState;

pub const STATUS_REPO_UNAVAILABLE: &str = "repo_unavailable";
pub const STATUS_BROKEN: &str = "broken";

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum NewTaskField {
    Repo,
    Branch,
    Base,
    Title,
    EnsureBaseUpToDate,
    Create,
    Cancel,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct NewTaskDialogState {
    pub repo_idx: usize,
    pub repo_input: String,
    pub branch_input: String,
    pub base_input: String,
    pub title_input: String,
    pub ensure_base_up_to_date: bool,
    pub loading_message: Option<String>,
    pub focused_field: NewTaskField,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum NewProjectField {
    Name,
    Create,
    Cancel,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct NewProjectDialogState {
    pub name_input: String,
    pub focused_field: NewProjectField,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ErrorDialogState {
    pub title: String,
    pub detail: String,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ConfirmQuitDialogState {
    pub active_session_count: usize,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum DeleteTaskField {
    KillTmux,
    RemoveWorktree,
    DeleteBranch,
    Delete,
    Cancel,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct DeleteTaskDialogState {
    pub task_id: Uuid,
    pub task_title: String,
    pub task_branch: String,
    pub kill_tmux: bool,
    pub remove_worktree: bool,
    pub delete_branch: bool,
    pub focused_field: DeleteTaskField,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct MoveTaskDialogState {
    pub category_idx: usize,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum CategoryInputField {
    Name,
    Confirm,
    Cancel,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum CategoryInputMode {
    Add,
    Rename,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum View {
    ProjectList,
    Board,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct CategoryInputDialogState {
    pub mode: CategoryInputMode,
    pub category_id: Option<Uuid>,
    pub name_input: String,
    pub focused_field: CategoryInputField,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum DeleteCategoryField {
    Delete,
    Cancel,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct DeleteCategoryDialogState {
    pub category_id: Uuid,
    pub category_name: String,
    pub task_count: usize,
    pub focused_field: DeleteCategoryField,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum WorktreeNotFoundField {
    Recreate,
    MarkBroken,
    Cancel,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct WorktreeNotFoundDialogState {
    pub task_id: Uuid,
    pub task_title: String,
    pub focused_field: WorktreeNotFoundField,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct RepoUnavailableDialogState {
    pub task_title: String,
    pub repo_path: String,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum ContextMenuItem {
    Attach,
    Delete,
    Move,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ContextMenuState {
    pub position: (u16, u16),
    pub task_id: Uuid,
    pub task_column: usize,
    pub items: Vec<ContextMenuItem>,
    pub selected_index: usize,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ViewMode {
    Kanban,
    SidePanel,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum TodoVisualizationMode {
    Summary,
    Checklist,
}

impl TodoVisualizationMode {
    pub fn as_str(self) -> &'static str {
        match self {
            TodoVisualizationMode::Summary => "summary",
            TodoVisualizationMode::Checklist => "checklist",
        }
    }

    pub fn cycle(self) -> Self {
        match self {
            TodoVisualizationMode::Summary => TodoVisualizationMode::Checklist,
            TodoVisualizationMode::Checklist => TodoVisualizationMode::Summary,
        }
    }
}

impl FromStr for TodoVisualizationMode {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "summary" => Ok(Self::Summary),
            "checklist" | "plan" => Ok(Self::Checklist),
            _ => Err(()),
        }
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq)]
pub enum ActiveDialog {
    None,
    NewTask(NewTaskDialogState),
    CommandPalette(CommandPaletteState),
    NewProject(NewProjectDialogState),
    CategoryInput(CategoryInputDialogState),
    DeleteCategory(DeleteCategoryDialogState),
    Error(ErrorDialogState),
    DeleteTask(DeleteTaskDialogState),
    MoveTask(MoveTaskDialogState),
    WorktreeNotFound(WorktreeNotFoundDialogState),
    RepoUnavailable(RepoUnavailableDialogState),
    ConfirmQuit(ConfirmQuitDialogState),
    Help,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct DesiredTaskState {
    pub expected_session_name: Option<String>,
    pub repo_available: bool,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ObservedTaskState {
    pub repo_available: bool,
    pub session_exists: bool,
    pub session_status: Option<crate::types::SessionStatus>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum AttachTaskResult {
    Attached,
    WorktreeNotFound,
    RepoUnavailable,
}

#[derive(Debug, Clone)]
pub struct CreateTaskOutcome {
    pub warning: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::TodoVisualizationMode;
    use std::str::FromStr;

    #[test]
    fn todo_visualization_mode_cycles_between_values() {
        assert_eq!(
            TodoVisualizationMode::Summary.cycle(),
            TodoVisualizationMode::Checklist
        );
        assert_eq!(
            TodoVisualizationMode::Checklist.cycle(),
            TodoVisualizationMode::Summary
        );
    }

    #[test]
    fn todo_visualization_mode_parses_supported_values() {
        assert_eq!(
            TodoVisualizationMode::from_str("summary"),
            Ok(TodoVisualizationMode::Summary)
        );
        assert_eq!(
            TodoVisualizationMode::from_str("checklist"),
            Ok(TodoVisualizationMode::Checklist)
        );
        assert_eq!(
            TodoVisualizationMode::from_str("plan"),
            Ok(TodoVisualizationMode::Checklist)
        );
    }
}
