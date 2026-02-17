//! Application state types for dialogs and UI components

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
    Settings,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum SettingsSection {
    Theme,
    Keybindings,
    General,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct SettingsViewState {
    pub active_section: SettingsSection,
    pub general_selected_field: usize,
    pub previous_view: View,
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
