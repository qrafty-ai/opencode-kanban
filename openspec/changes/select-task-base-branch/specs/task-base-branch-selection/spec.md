## ADDED Requirements

### Requirement: Select a source branch in the ordinary task dialog
The system SHALL let a user creating an ordinary task select a source branch from local branches and `origin/*` remote-tracking branches. The source-branch control SHALL remain editable so the user can provide an arbitrary Git ref.

#### Scenario: Select a local source branch
- **WHEN** the user selects a local branch as the source and submits a valid new task
- **THEN** the system creates the task's new local branch and worktree from that local branch

#### Scenario: Select a remote source branch
- **WHEN** the user selects an `origin/*` branch as the source and submits a valid new task
- **THEN** the system creates the task's new local branch and worktree from that remote-tracking branch

#### Scenario: Type an arbitrary source ref
- **WHEN** the user enters a source ref not present in the picker
- **THEN** the system attempts to create the task from that ref using normal Git ref resolution

### Requirement: Track explicitly selected remote sources
The system SHALL configure a task branch created from a selected `origin/*` source branch to track that same remote-tracking branch.

#### Scenario: Create a differently named local tracking branch
- **WHEN** the user creates local branch `review-widget` from selected source `origin/feature/widget`
- **THEN** `review-widget` has `origin/feature/widget` configured as its upstream

#### Scenario: Create from a local source branch
- **WHEN** the user creates a task branch from a selected local source branch
- **THEN** the system does not add an upstream relationship solely because of that selection

### Requirement: Refresh and resolve remote source branches
The system SHALL refresh `origin` and resolve an explicitly selected remote source before creating its worktree. The system SHALL present an actionable error and not create a task when the refresh or source resolution fails.

#### Scenario: Remote source is available after refresh
- **WHEN** a selected `origin/*` source exists after the refresh
- **THEN** the system proceeds with task and worktree creation

#### Scenario: Remote source cannot be resolved
- **WHEN** a selected remote source is missing after refresh
- **THEN** the system leaves the task uncreated and explains that the source branch is unavailable

#### Scenario: Fetch fails
- **WHEN** refreshing `origin` for a selected remote source fails
- **THEN** the system leaves the task uncreated and displays the fetch failure
