//! Application state types for dialogs and UI components

use std::path::PathBuf;
use std::str::FromStr;

use uuid::Uuid;

use crate::command_palette::CommandPaletteState;

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum NewTaskField {
    Repo,
    UseExistingDirectory,
    Branch,
    Base,
    ExistingDirectory,
    Title,
    EnsureBaseUpToDate,
    Create,
    Cancel,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum RepoSuggestionKind {
    KnownRepo { repo_idx: usize },
    FolderPath,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum RepoPickerTarget {
    Repo,
    ExistingDirectory,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct RepoSuggestionItem {
    pub label: String,
    pub value: String,
    pub kind: RepoSuggestionKind,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct RepoPickerDialogState {
    pub target: RepoPickerTarget,
    pub query: String,
    pub selected_index: usize,
    pub suggestions: Vec<RepoSuggestionItem>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct NewTaskDialogState {
    pub repo_idx: usize,
    pub repo_input: String,
    pub repo_picker: Option<RepoPickerDialogState>,
    pub use_existing_directory: bool,
    pub existing_dir_input: String,
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

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum RenameProjectField {
    Name,
    Confirm,
    Cancel,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct RenameProjectDialogState {
    pub name_input: String,
    pub focused_field: RenameProjectField,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct DeleteProjectDialogState {
    pub project_name: String,
    pub project_path: PathBuf,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum RenameRepoField {
    Name,
    Confirm,
    Cancel,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct RenameRepoDialogState {
    pub repo_id: Uuid,
    pub name_input: String,
    pub focused_field: RenameRepoField,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct DeleteRepoDialogState {
    pub repo_id: Uuid,
    pub repo_name: String,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ErrorDialogState {
    pub title: String,
    pub detail: String,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ConfirmQuitDialogState {
    pub active_session_count: usize,
    pub focused_field: ConfirmCancelField,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ConfirmCancelField {
    Confirm,
    Cancel,
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
    pub confirm_destructive: bool,
    pub focused_field: DeleteTaskField,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ArchiveTaskDialogState {
    pub task_id: Uuid,
    pub task_title: String,
    pub focused_field: ConfirmCancelField,
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

pub const CATEGORY_COLOR_PALETTE: [Option<&str>; 7] = [
    None,
    Some("primary"),
    Some("secondary"),
    Some("tertiary"),
    Some("success"),
    Some("warning"),
    Some("danger"),
];

pub fn normalize_category_color_key(color: Option<&str>) -> Option<&'static str> {
    let value = color?.trim();
    if value.is_empty() {
        return None;
    }

    match value.to_ascii_lowercase().as_str() {
        "primary" | "cyan" => Some("primary"),
        "secondary" | "magenta" => Some("secondary"),
        "tertiary" | "blue" => Some("tertiary"),
        "success" | "green" => Some("success"),
        "warning" | "yellow" => Some("warning"),
        "danger" | "red" => Some("danger"),
        _ => None,
    }
}

pub fn category_color_label(color: Option<&str>) -> &'static str {
    match (color, normalize_category_color_key(color)) {
        (None, _) => "Default",
        (Some(_), Some("primary")) => "Primary",
        (Some(_), Some("secondary")) => "Secondary",
        (Some(_), Some("tertiary")) => "Tertiary",
        (Some(_), Some("success")) => "Success",
        (Some(_), Some("warning")) => "Warning",
        (Some(_), Some("danger")) => "Danger",
        (Some(_), Some(_)) => "Custom",
        (Some(_), None) => "Custom",
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum CategoryColorField {
    Palette,
    Confirm,
    Cancel,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct CategoryColorDialogState {
    pub category_id: Uuid,
    pub category_name: String,
    pub selected_index: usize,
    pub focused_field: CategoryColorField,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum View {
    ProjectList,
    Board,
    Settings,
    Archive,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum SettingsSection {
    General,
    CategoryColors,
    Keybindings,
    Repos,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct SettingsViewState {
    pub active_section: SettingsSection,
    pub general_selected_field: usize,
    pub category_color_selected: usize,
    pub repos_selected_field: usize,
    pub previous_view: View,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct CategoryInputDialogState {
    pub mode: CategoryInputMode,
    pub category_id: Option<Uuid>,
    pub name_input: String,
    pub focused_field: CategoryInputField,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct DeleteCategoryDialogState {
    pub category_id: Uuid,
    pub category_name: String,
    pub task_count: usize,
    pub focused_field: ConfirmCancelField,
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
    RenameProject(RenameProjectDialogState),
    DeleteProject(DeleteProjectDialogState),
    CategoryInput(CategoryInputDialogState),
    CategoryColor(CategoryColorDialogState),
    DeleteCategory(DeleteCategoryDialogState),
    Error(ErrorDialogState),
    ArchiveTask(ArchiveTaskDialogState),
    DeleteTask(DeleteTaskDialogState),
    MoveTask(MoveTaskDialogState),
    WorktreeNotFound(WorktreeNotFoundDialogState),
    RepoUnavailable(RepoUnavailableDialogState),
    ConfirmQuit(ConfirmQuitDialogState),
    RenameRepo(RenameRepoDialogState),
    DeleteRepo(DeleteRepoDialogState),
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

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum DetailFocus {
    List,
    Details,
    Log,
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
