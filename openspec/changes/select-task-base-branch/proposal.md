## Why

Creating a task currently relies on manually typing a base ref. Users cannot discover existing local or remote branches in the task dialog, and a task created from a remote branch does not automatically receive its upstream relationship.

## What Changes

- Add branch selection to the existing New Task dialog; no new task type or workflow is introduced.
- Let users choose a local branch or an `origin/*` remote-tracking branch as the source for the task's new local branch.
- Preserve typed source refs as an advanced fallback.
- When the source is remote, configure the created local branch to track that remote branch.
- Refresh remote branch choices from `origin` and surface fetch or resolution failures in the dialog.

## Capabilities

### New Capabilities
- `task-base-branch-selection`: Select a local or remote branch as the starting point for a new task branch, with automatic upstream tracking for remote sources.

### Modified Capabilities

None.

## Impact

- Affects the New Task state, dialog navigation and rendering, task-creation workflow, and Git branch/worktree helpers.
- Adds unit and integration coverage for local sources, remote sources, tracking configuration, and source-resolution failures.
