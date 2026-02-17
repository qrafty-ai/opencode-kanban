//! Semantic user action messages for tui-realm UI components.
//!
//! This module defines the `Msg` enum representing user intentions rather than
//! raw hardware events. Raw input (KeyEvent, MouseEvent) is handled by the
//! input layer and converted to semantic messages here.
//!
//! # Mapping from Legacy Messages
//!
//! The legacy `src/app/messages.rs` contained 45+ variants including raw input
//! events (Key(KeyEvent), Mouse(MouseEvent)). This semantic enum consolidates
//! them into meaningful user actions:
//!
//! | Legacy Group | New Semantic Variants |
//! |--------------|----------------------|
//! | Navigation (NavigateLeft, NavigateRight, SelectUp, SelectDown) | Navigation variant group |
//! | Dialog open (OpenNewTaskDialog, OpenDeleteTaskDialog, etc.) | DialogOpen variant with DialogType |
//! | Focus (FocusColumn, FocusNewTaskField) | Focus variant with Target |
//! | Selection (SelectTask, SelectProject) | Select variant with parameters |
//! | Task actions (CreateTask, DeleteTask, ConfirmDeleteTask) | Task* variant group |
//! | Category actions (SubmitCategoryInput, ConfirmDeleteCategory) | Category* variant group |
//! | Project actions (SwitchToProjectList, CreateProject) | Project* variant group |
//! | System (Tick, Resize, ConfirmQuit) | System* variant group |
//! | Raw input (Key, Mouse) | REMOVED - handled by input layer |
//!
//! # Design Principles
//!
//! 1. **Semantic over syntactic**: Messages represent user intent, not key presses
//! 2. **No raw events**: KeyEvent/MouseEvent belong in input layer, not here
//! 3. **Coarse over fine**: Consolidate similar actions rather than 1:1 mapping
//! 4. **Component-agnostic**: Messages are UI-layer, not tied to specific components

use super::ComponentId;

/// Semantic user action messages.
///
/// Each variant represents a user intention or system event at a semantic level.
/// These messages drive the tui-realm Model<Msg> update cycle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Msg {
    // =========================================================================
    // Navigation - moving focus between UI regions
    // =========================================================================
    /// Move focus left (previous column or element)
    ///
    /// Legacy: `NavigateLeft`
    NavigateLeft,

    /// Move focus right (next column or element)
    ///
    /// Legacy: `NavigateRight`
    NavigateRight,

    /// Move selection up (previous item in list/column)
    ///
    /// Legacy: `SelectUp`
    SelectUp,

    /// Move selection down (next item in list/column)
    ///
    /// Legacy: `SelectDown`
    SelectDown,

    // =========================================================================
    // Task Actions - task CRUD and manipulation
    // =========================================================================
    /// Open dialog to create a new task
    ///
    /// Legacy: `OpenNewTaskDialog`
    OpenNewTaskDialog,

    /// Confirm task creation (submit form)
    ///
    /// Legacy: `CreateTask`
    CreateTask,

    /// Open delete confirmation for selected task
    ///
    /// Legacy: `OpenDeleteTaskDialog`
    OpenDeleteTaskDialog,

    /// Confirm task deletion
    ///
    /// Legacy: `ConfirmDeleteTask`
    ConfirmDeleteTask,

    /// Toggle "kill tmux session" option in delete dialog
    ///
    /// Legacy: `DeleteTaskToggleKillTmux`
    ToggleDeleteKillTmux,

    /// Toggle "remove worktree" option in delete dialog
    ///
    /// Legacy: `DeleteTaskToggleRemoveWorktree`
    ToggleDeleteRemoveWorktree,

    /// Toggle "delete branch" option in delete dialog
    ///
    /// Legacy: `DeleteTaskToggleDeleteBranch`
    ToggleDeleteBranch,

    /// Attach to selected task (launch OpenCode/tmux)
    ///
    /// Legacy: `AttachSelectedTask`
    AttachTask,

    /// Recreate missing worktree for task
    ///
    /// Legacy: `WorktreeNotFoundRecreate`
    RecreateWorktree,

    /// Mark task as broken (worktree unavailable)
    ///
    /// Legacy: `WorktreeNotFoundMarkBroken`
    MarkWorktreeBroken,

    // =========================================================================
    // Category Actions - category CRUD
    // =========================================================================
    /// Open dialog to add a new category
    ///
    /// Legacy: `OpenAddCategoryDialog`
    OpenAddCategoryDialog,

    /// Open dialog to rename a category
    ///
    /// Legacy: `OpenRenameCategoryDialog`
    OpenRenameCategoryDialog,

    /// Open dialog to delete a category
    ///
    /// Legacy: `OpenDeleteCategoryDialog`
    OpenDeleteCategoryDialog,

    /// Submit category input (create/rename)
    ///
    /// Legacy: `SubmitCategoryInput`
    SubmitCategoryInput,

    /// Confirm category deletion
    ///
    /// Legacy: `ConfirmDeleteCategory`
    ConfirmDeleteCategory,

    /// Cycle category color
    ///
    /// Legacy: `CycleCategoryColor(usize)`
    CycleCategoryColor(usize),

    // =========================================================================
    // Category Movement - reorder tasks within/between categories
    // =========================================================================
    /// Move task left to previous category
    ///
    /// Legacy: `MoveTaskLeft`
    MoveTaskLeft,

    /// Move task right to next category
    ///
    /// Legacy: `MoveTaskRight`
    MoveTaskRight,

    /// Move task up within category
    ///
    /// Legacy: `MoveTaskUp`
    MoveTaskUp,

    /// Move task down within category
    ///
    /// Legacy: `MoveTaskDown`
    MoveTaskDown,

    // =========================================================================
    // Dialog Actions - generic dialog handling
    // =========================================================================
    /// Dismiss current dialog without action
    ///
    /// Legacy: `DismissDialog`
    DismissDialog,

    /// Submit current dialog with current values
    ///
    /// Legacy: `ExecuteCommand(String)` (for command palette), form submission
    SubmitDialog,

    /// Confirm dialog action
    ///
    /// Legacy: various confirm actions
    ConfirmAction,

    /// Cancel dialog action
    ///
    /// Legacy: focus on Cancel button variants
    CancelAction,

    /// Focus a specific field in current dialog
    ///
    /// Legacy: `FocusNewTaskField`, `FocusCategoryInputField`, `FocusDeleteTaskField`
    FocusField(DialogField),

    /// Toggle checkbox in dialog
    ///
    /// Legacy: `ToggleNewTaskCheckbox`, `ToggleDeleteTaskCheckbox`
    ToggleCheckbox(DialogField),

    /// Focus a specific dialog button
    ///
    /// Legacy: `FocusDialogButton(String)`
    FocusButton(String),

    // =========================================================================
    // Project Actions - project management
    // =========================================================================
    /// Switch to project list view
    ///
    /// Legacy: `SwitchToProjectList`
    OpenProjectList,

    /// Switch to board view for a specific project
    ///
    /// Legacy: `SwitchToBoard(PathBuf)`
    SwitchToProject(String),

    /// Select a project in project list
    ///
    /// Legacy: `SelectProject(usize)`
    SelectProject(usize),

    /// Open dialog to create new project
    ///
    /// Legacy: `OpenNewProjectDialog`
    OpenNewProjectDialog,

    /// Create new project
    ///
    /// Legacy: `CreateProject`
    CreateProject,

    /// Move selection up in project list
    ///
    /// Legacy: `ProjectListSelectUp`
    ProjectListSelectUp,

    /// Move selection down in project list
    ///
    /// Legacy: `ProjectListSelectDown`
    ProjectListSelectDown,

    /// Confirm project selection
    ///
    /// Legacy: `ProjectListConfirm`
    ProjectListConfirm,

    // =========================================================================
    // Command Palette
    // =========================================================================
    /// Open command palette
    ///
    /// Legacy: `OpenCommandPalette`
    OpenCommandPalette,

    /// Execute a command from palette
    ///
    /// Legacy: `ExecuteCommand(String)`
    ExecuteCommand(String),

    /// Select item in command palette list
    ///
    /// Legacy: `SelectCommandPaletteItem(usize)`
    SelectCommandPaletteItem(usize),

    // =========================================================================
    // Focus and Selection - UI state management
    // =========================================================================
    /// Focus a specific column by index
    ///
    /// Legacy: `FocusColumn(usize)`
    FocusColumn(usize),

    /// Select a specific task
    ///
    /// Legacy: `SelectTask(usize, usize)` - column, task
    SelectTask { column: usize, task: usize },

    /// Select task in side panel
    ///
    /// Legacy: `SelectTaskInSidePanel(usize)`
    SelectTaskInSidePanel(usize),

    // =========================================================================
    // View Modes - UI display state
    // =========================================================================
    /// Toggle between kanban and side panel view
    ///
    /// Legacy: `ViewMode` changes via keyboard
    ToggleViewMode,

    // =========================================================================
    // System Events - application lifecycle
    // =========================================================================
    /// Tick event for periodic updates
    ///
    /// Legacy: `Tick`
    Tick,

    /// Terminal resize event
    ///
    /// Legacy: `Resize(u16, u16)`
    Resize { width: u16, height: u16 },

    /// Confirm quit application
    ///
    /// Legacy: `ConfirmQuit`
    ConfirmQuit,

    /// Cancel quit
    ///
    /// Legacy: `CancelQuit`
    CancelQuit,

    /// Open quit confirmation dialog
    ///
    /// Legacy: triggered by quit keybinding with active sessions
    OpenQuitDialog,

    // =========================================================================
    // Error Handling - error display and dismissal
    // =========================================================================
    /// Dismiss repo unavailable notification
    ///
    /// Legacy: `RepoUnavailableDismiss`
    DismissRepoError,

    /// Show error dialog with message
    ///
    /// Legacy: `Error` dialog variants
    ShowError(String),
}

// ===========================================================================
// Supporting Types
// ===========================================================================

/// Dialog field identifiers for focus/toggle operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DialogField {
    // New task fields
    Repo,
    Branch,
    Base,
    Title,
    EnsureBaseUpToDate,
    Create,
    Cancel,

    // Delete task fields
    KillTmux,
    RemoveWorktree,
    DeleteBranch,
    Delete,
    CancelDelete,

    // Category fields
    Name,
    Confirm,
    CancelCategory,

    // Worktree not found fields
    Recreate,
    MarkBroken,
    CancelWorktree,
}

/// Dialog type for OpenDialog message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DialogType {
    NewTask,
    DeleteTask,
    CategoryInput,
    DeleteCategory,
    NewProject,
    ConfirmQuit,
    Help,
    WorktreeNotFound,
    RepoUnavailable,
    Error,
    MoveTask,
}

impl DialogType {
    /// Get corresponding ComponentId for this dialog type.
    pub fn to_component_id(&self) -> ComponentId {
        match self {
            DialogType::NewTask => ComponentId::NewTask,
            DialogType::DeleteTask => ComponentId::DeleteTask,
            DialogType::CategoryInput => ComponentId::CategoryInput,
            DialogType::DeleteCategory => ComponentId::DeleteCategory,
            DialogType::NewProject => ComponentId::NewProject,
            DialogType::ConfirmQuit => ComponentId::ConfirmQuit,
            DialogType::Help => ComponentId::Help,
            DialogType::WorktreeNotFound => ComponentId::WorktreeNotFound,
            DialogType::RepoUnavailable => ComponentId::RepoUnavailable,
            DialogType::Error => ComponentId::Error,
            DialogType::MoveTask => ComponentId::MoveTask,
        }
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod msg {
    use super::*;

    /// Test that all Msg variants are constructible.
    #[test]
    fn constructible() {
        // Navigation
        let _ = Msg::NavigateLeft;
        let _ = Msg::NavigateRight;
        let _ = Msg::SelectUp;
        let _ = Msg::SelectDown;

        // Task actions
        let _ = Msg::OpenNewTaskDialog;
        let _ = Msg::CreateTask;
        let _ = Msg::OpenDeleteTaskDialog;
        let _ = Msg::ConfirmDeleteTask;
        let _ = Msg::ToggleDeleteKillTmux;
        let _ = Msg::ToggleDeleteRemoveWorktree;
        let _ = Msg::ToggleDeleteBranch;
        let _ = Msg::AttachTask;
        let _ = Msg::RecreateWorktree;
        let _ = Msg::MarkWorktreeBroken;

        // Category actions
        let _ = Msg::OpenAddCategoryDialog;
        let _ = Msg::OpenRenameCategoryDialog;
        let _ = Msg::OpenDeleteCategoryDialog;
        let _ = Msg::SubmitCategoryInput;
        let _ = Msg::ConfirmDeleteCategory;
        let _ = Msg::CycleCategoryColor(0);

        // Category movement
        let _ = Msg::MoveTaskLeft;
        let _ = Msg::MoveTaskRight;
        let _ = Msg::MoveTaskUp;
        let _ = Msg::MoveTaskDown;

        // Dialog actions
        let _ = Msg::DismissDialog;
        let _ = Msg::SubmitDialog;
        let _ = Msg::ConfirmAction;
        let _ = Msg::CancelAction;
        let _ = Msg::FocusField(DialogField::Repo);
        let _ = Msg::ToggleCheckbox(DialogField::Title);
        let _ = Msg::FocusButton("confirm".to_string());

        // Project actions
        let _ = Msg::OpenProjectList;
        let _ = Msg::SwitchToProject("main".to_string());
        let _ = Msg::SelectProject(0);
        let _ = Msg::OpenNewProjectDialog;
        let _ = Msg::CreateProject;
        let _ = Msg::ProjectListSelectUp;
        let _ = Msg::ProjectListSelectDown;
        let _ = Msg::ProjectListConfirm;

        // Command palette
        let _ = Msg::OpenCommandPalette;
        let _ = Msg::ExecuteCommand("test".to_string());
        let _ = Msg::SelectCommandPaletteItem(0);

        // Focus and selection
        let _ = Msg::FocusColumn(0);
        let _ = Msg::SelectTask { column: 0, task: 0 };
        let _ = Msg::SelectTaskInSidePanel(0);

        // View modes
        let _ = Msg::ToggleViewMode;

        // System events
        let _ = Msg::Tick;
        let _ = Msg::Resize {
            width: 80,
            height: 24,
        };
        let _ = Msg::ConfirmQuit;
        let _ = Msg::CancelQuit;
        let _ = Msg::OpenQuitDialog;

        // Error handling
        let _ = Msg::DismissRepoError;
        let _ = Msg::ShowError("test error".to_string());
    }

    /// Test Msg derives Clone correctly.
    #[test]
    fn clone_behavior() {
        let original = Msg::SelectTask { column: 1, task: 2 };
        let cloned = original.clone();
        assert_eq!(original, cloned);
    }

    /// Test DialogField derives correctly.
    #[test]
    fn dialog_field_constructible() {
        let _ = DialogField::Repo;
        let _ = DialogField::Branch;
        let _ = DialogField::Title;
        let _ = DialogField::KillTmux;
        let _ = DialogField::Name;
    }

    /// Test DialogType conversion to ComponentId.
    #[test]
    fn dialog_type_to_component_id() {
        assert_eq!(DialogType::NewTask.to_component_id(), ComponentId::NewTask);
        assert_eq!(
            DialogType::DeleteTask.to_component_id(),
            ComponentId::DeleteTask
        );
        assert_eq!(DialogType::Help.to_component_id(), ComponentId::Help);
    }
}
