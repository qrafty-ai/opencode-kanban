//! tui-realm UI components and types
//!
//! This module provides the ComponentId enum for all UI components in the
//! tui-realm migration, including core board components and dialog components.

#[cfg(test)]
pub mod tests;

pub mod application;
pub mod components;
pub mod messages;
pub mod model;

/// Component identifier enum for tui-realm Application.
///
/// Each variant represents a unique component in the UI hierarchy.
/// Tuple variants (e.g., KanbanColumn(usize)) are used for components
/// that have multiple instances.
///
/// # Variants
///
/// ## Core Board Components
/// - `ProjectList`: Project selection list
/// - `KanbanColumn(usize)`: Kanban board column (indexed)
/// - `TaskCard(usize)`: Task card within a column (indexed)
/// - `SidePanel`: Task detail side panel
/// - `ContextMenu`: Right-click context menu
/// - `Footer`: Status bar / keyboard hints
/// - `CommandPalette`: Fuzzy command search overlay
///
/// ## Dialog Components
/// - `NewTask`: Create new task dialog
/// - `DeleteTask`: Delete task confirmation dialog
/// - `CategoryInput`: Add/rename category dialog
/// - `DeleteCategory`: Delete category confirmation
/// - `NewProject`: Create new project dialog
/// - `ConfirmQuit`: Quit confirmation with active sessions
/// - `Help`: Keyboard shortcuts overlay
/// - `WorktreeNotFound`: Handle missing worktree dialog
/// - `RepoUnavailable`: Handle unavailable repo dialog
/// - `Error`: Generic error display dialog
/// - `MoveTask`: Move task between categories (placeholder)
///
/// # Notes
///
/// - The plan references 15 dialogs, but ActiveDialog has 12 functional types.
///   MoveTask is a placeholder in the current implementation.
/// - CommandPalette appears as both a core component and dialog - it's the same
///   component used in different contexts.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum ComponentId {
    // Core board components
    ProjectList,
    KanbanColumn(usize),
    TaskCard(usize),
    SidePanel,
    ContextMenu,
    Footer,
    CommandPalette,

    // Dialog components
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

#[cfg(test)]
mod component_id {
    use super::ComponentId;
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    /// Test that all ComponentId variants are constructible.
    #[test]
    fn constructible() {
        // Core components - unit variants
        let _ = ComponentId::ProjectList;
        let _ = ComponentId::SidePanel;
        let _ = ComponentId::ContextMenu;
        let _ = ComponentId::Footer;
        let _ = ComponentId::CommandPalette;

        // Core components - tuple variants
        let _ = ComponentId::KanbanColumn(0);
        let _ = ComponentId::KanbanColumn(5);
        let _ = ComponentId::TaskCard(0);
        let _ = ComponentId::TaskCard(10);

        // Dialog components
        let _ = ComponentId::NewTask;
        let _ = ComponentId::DeleteTask;
        let _ = ComponentId::CategoryInput;
        let _ = ComponentId::DeleteCategory;
        let _ = ComponentId::NewProject;
        let _ = ComponentId::ConfirmQuit;
        let _ = ComponentId::Help;
        let _ = ComponentId::WorktreeNotFound;
        let _ = ComponentId::RepoUnavailable;
        let _ = ComponentId::Error;
        let _ = ComponentId::MoveTask;
    }

    /// Test that ComponentId implements Clone correctly.
    #[test]
    fn clone_behavior() {
        let original = ComponentId::KanbanColumn(3);
        let cloned = original.clone();

        assert_eq!(original, cloned);
        assert!(std::ptr::eq(&original, &cloned) || original == cloned);
    }

    /// Test that ComponentId implements Hash correctly.
    #[test]
    fn hash_behavior() {
        let mut hasher1 = DefaultHasher::new();
        let mut hasher2 = DefaultHasher::new();

        ComponentId::KanbanColumn(5).hash(&mut hasher1);
        ComponentId::KanbanColumn(5).hash(&mut hasher2);

        assert_eq!(hasher1.finish(), hasher2.finish());

        // Different variants should have different hashes (statistically)
        let mut hasher3 = DefaultHasher::new();
        ComponentId::ProjectList.hash(&mut hasher3);
        assert_ne!(hasher1.finish(), hasher3.finish());
    }

    /// Test equality between tuple variant instances.
    #[test]
    fn tuple_variant_equality() {
        assert_eq!(ComponentId::KanbanColumn(0), ComponentId::KanbanColumn(0));
        assert_ne!(ComponentId::KanbanColumn(0), ComponentId::KanbanColumn(1));
        assert_eq!(ComponentId::TaskCard(42), ComponentId::TaskCard(42));
        assert_ne!(ComponentId::TaskCard(0), ComponentId::TaskCard(1));
    }
}
