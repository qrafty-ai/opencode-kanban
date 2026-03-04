# opencode-kanban — Rewrite Spec

## Goal
Recreate `opencode-kanban` as a Rust application that manages project-scoped kanban boards backed by SQLite, with each task optionally bound to a Git worktree, a tmux session, and an OpenCode session. The rewrite must preserve:
- tmux-first workflows
- one SQLite DB per project
- board plus detail-oriented TUI
- OpenCode status/TODO/message polling
- lightweight CLI for project-scoped automation

Core behavioral references: [README.md](/Users/Artemis/Arc/opencode-kanban/README.md#L8), [src/main.rs](/Users/Artemis/Arc/opencode-kanban/src/main.rs#L102), [src/app/core.rs](/Users/Artemis/Arc/opencode-kanban/src/app/core.rs#L123).

## Non-Negotiable Product Behavior
- Starting outside tmux must move the user into a tmux session before the TUI runs. Source: [src/main.rs](/Users/Artemis/Arc/opencode-kanban/src/main.rs#L174).
- Each project is stored as a standalone SQLite file under the local data directory. Source: [src/projects.rs](/Users/Artemis/Arc/opencode-kanban/src/projects.rs#L16).
- Tasks are branch-scoped per repo and persist tmux/OpenCode binding metadata. Sources: [src/db/mod.rs](/Users/Artemis/Arc/opencode-kanban/src/db/mod.rs#L166), [src/db/mod.rs](/Users/Artemis/Arc/opencode-kanban/src/db/mod.rs#L836).
- The TUI must support project list, board, settings, and archive views. Source: [src/app/state.rs](/Users/Artemis/Arc/opencode-kanban/src/app/state.rs#L255).
- Board mode must support both column kanban and side-panel detail layouts. Source: [src/app/state.rs](/Users/Artemis/Arc/opencode-kanban/src/app/state.rs#L333).
- Status/TODO/message data must refresh asynchronously from the OpenCode server without blocking rendering. Source: [src/app/polling.rs](/Users/Artemis/Arc/opencode-kanban/src/app/polling.rs#L23).
- The CLI must remain project-scoped and expose task/category CRUD-ish operations. Source: [src/cli/mod.rs](/Users/Artemis/Arc/opencode-kanban/src/cli/mod.rs#L27).

## Suggested Rewrite Architecture
Preserve the existing conceptual split, but make the boundaries explicit:

### 1. `domain`
Responsibility
- Own pure data contracts and domain rules.

Modules
- `domain::types`
- `domain::status`
- `domain::settings_model`

Data contracts
- `Repo`
- `Category`
- `Task`
- `SessionTodoItem`
- `SessionMessageItem`
- `SessionStatus`
- enums for `SessionState`, `SessionStatusSource`, `View`, `ViewMode`, `TodoVisualizationMode`

Why
- These types are already stable and widely shared. A rewrite should keep them shell- and UI-agnostic. Sources: [src/types.rs](/Users/Artemis/Arc/opencode-kanban/src/types.rs#L6), [src/app/state.rs](/Users/Artemis/Arc/opencode-kanban/src/app/state.rs#L255).

### 2. `persistence`
Responsibility
- Own schema creation, migrations, CRUD, and query helpers.

Modules
- `persistence::db`
- `persistence::projects`
- optional `persistence::migrations`

Data contracts
- SQL tables: `repos`, `categories`, `tasks`, `command_frequency`
- migration/backfill rules for status/source/archive/category slug/color

Why
- The current DB layer is already the single source of truth and should remain the persistence boundary. Sources: [src/db/mod.rs](/Users/Artemis/Arc/opencode-kanban/src/db/mod.rs#L805), [src/projects.rs](/Users/Artemis/Arc/opencode-kanban/src/projects.rs#L67).

### 3. `integrations::git`
Responsibility
- Wrap git operations and repo metadata detection.

Must implement
- detect default branch
- fetch origin
- validate branch names
- create/remove worktrees
- delete branch
- compute change summary
- derive worktree path

Data contracts
- `Branch`
- `GitChangeSummary`

Why
- This is a natural shell boundary and can stay command-based in the rewrite. Sources: [src/git/mod.rs](/Users/Artemis/Arc/opencode-kanban/src/git/mod.rs#L7), [src/git/mod.rs](/Users/Artemis/Arc/opencode-kanban/src/git/mod.rs#L215).

### 4. `integrations::tmux`
Responsibility
- Wrap tmux session management, client switching, popups, and terminal opening.

Must implement
- check/install assumptions
- create/kill/list sessions
- switch client
- display popup/message
- derive session names
- terminal-launch attach

Data contracts
- `TmuxSession`
- `PopupThemeStyle`

Why
- Existing design cleanly isolates tmux shelling and should be preserved. Sources: [src/tmux/mod.rs](/Users/Artemis/Arc/opencode-kanban/src/tmux/mod.rs#L28), [src/tmux/mod.rs](/Users/Artemis/Arc/opencode-kanban/src/tmux/mod.rs#L57), [src/tmux/mod.rs](/Users/Artemis/Arc/opencode-kanban/src/tmux/mod.rs#L294).

### 5. `integrations::opencode`
Responsibility
- Own OpenCode command integration, local server bootstrap, and HTTP contract parsing.

Must implement
- `opencode attach` command derivation
- launch/resume helpers
- directory-to-session lookup
- server bootstrap with health checks
- async status/todo/message/session-record HTTP client
- binding-state classification

Data contracts
- `OpenCodeBindingState`
- `OpenCodeServerState`
- `OpenCodeServerManager`
- `ServerStatusProvider`
- `SessionStatusMatch`
- `SessionRecord`

Why
- The current project has two OpenCode concerns: shell command execution and local HTTP polling. Keeping them together but internally split is appropriate. Sources: [src/opencode/mod.rs](/Users/Artemis/Arc/opencode-kanban/src/opencode/mod.rs#L25), [src/opencode/server.rs](/Users/Artemis/Arc/opencode-kanban/src/opencode/server.rs#L8), [src/opencode/status_server.rs](/Users/Artemis/Arc/opencode-kanban/src/opencode/status_server.rs#L16).

### 6. `application`
Responsibility
- Contain stateful workflows and background processes, but not rendering.

Modules
- `application::app_state`
- `application::messages`
- `application::workflows`
- `application::poller`
- `application::navigation`
- `application::settings_actions`

Must implement
- startup construction
- refresh board/project data
- task creation workflow
- attach workflow
- startup reconciliation
- side-panel/change-summary coordination
- command/task palette orchestration

Data contracts
- `App`
- all dialog state structs
- `Message`
- `AttachTaskResult`
- `CreateTaskOutcome`
- `SubagentTodoSummary`

Why
- This is the largest subsystem and should remain the orchestration layer above persistence/integrations. Sources: [src/app/core.rs](/Users/Artemis/Arc/opencode-kanban/src/app/core.rs#L20), [src/app/messages.rs](/Users/Artemis/Arc/opencode-kanban/src/app/messages.rs#L12), [src/app/workflows/create_task.rs](/Users/Artemis/Arc/opencode-kanban/src/app/workflows/create_task.rs#L39).

### 7. `ui`
Responsibility
- Pure-ish rendering and input mapping.

Modules
- `ui::render`
- `ui::components`
- `ui::interaction_map`
- `ui::realm_adapter`

Must implement
- top-level views
- dialogs
- overlays
- context menus
- task/detail/log panels
- interaction hitboxes

Why
- Current `ui.rs` is monolithic; rewrite should split by view/component, but keep rendering isolated from workflows. Sources: [src/ui.rs](/Users/Artemis/Arc/opencode-kanban/src/ui.rs#L45), [src/realm.rs](/Users/Artemis/Arc/opencode-kanban/src/realm.rs#L40).

### 8. `cli`
Responsibility
- Thin automation surface over the persistence/workflow layer.

Must implement
- `task list/create/move/archive/show`
- `category list/create/update/delete`
- text and JSON output modes
- stable machine-readable error codes

Why
- The CLI is intentionally small and should stay separate from the TUI event model. Source: [src/cli/mod.rs](/Users/Artemis/Arc/opencode-kanban/src/cli/mod.rs#L27).

## Required Data Contracts

### Persisted entities
`Repo`
- `id: Uuid`
- `path: String`
- `name: String`
- `default_base: Option<String>`
- `remote_url: Option<String>`
- `created_at: String`
- `updated_at: String`
Source: [src/types.rs](/Users/Artemis/Arc/opencode-kanban/src/types.rs#L6).

`Category`
- `id: Uuid`
- `slug: String`
- `name: String`
- `position: i64`
- `color: Option<String>`
- `created_at: String`
Source: [src/types.rs](/Users/Artemis/Arc/opencode-kanban/src/types.rs#L17).

`Task`
- identity/board: `id`, `title`, `repo_id`, `branch`, `category_id`, `position`
- tmux/worktree: `tmux_session_name`, `worktree_path`
- status: `tmux_status`, `status_source`, `status_fetched_at`, `status_error`
- OpenCode binding: `opencode_session_id`
- UI flags: `attach_overlay_shown`, `needs_inspection`
- archive: `archived`, `archived_at`
- timestamps: `created_at`, `updated_at`
Source: [src/types.rs](/Users/Artemis/Arc/opencode-kanban/src/types.rs#L27).

### Runtime/session entities
`SessionTodoItem`
- `content`
- `completed`

`SessionMessageItem`
- `message_type`
- `role`
- `content`
- `timestamp`

`SessionStatus`
- `state`
- `source`
- `fetched_at`
- `error`
Source: [src/types.rs](/Users/Artemis/Arc/opencode-kanban/src/types.rs#L52).

### UI/application entities
`App`
- must retain board data, project list state, dialogs, settings, caches, and poller/worker handles
- can be decomposed internally, but there must still be a single root interactive state object
Source: [src/app/core.rs](/Users/Artemis/Arc/opencode-kanban/src/app/core.rs#L20).

`Message`
- exhaustive event enum for keyboard/mouse/tick/window/system/application events
- rewrite can reduce surface area, but every user-visible action currently routes through this enum
Source: [src/app/messages.rs](/Users/Artemis/Arc/opencode-kanban/src/app/messages.rs#L12).

## Reconstruction Order
1. Rebuild the domain types and enums.
2. Rebuild SQLite schema/migrations and project-file lifecycle.
3. Rebuild git and tmux adapters.
4. Rebuild OpenCode command/server/status adapters.
5. Rebuild `App` startup and refresh logic.
6. Rebuild task creation and attach workflows.
7. Rebuild background poller and cache model.
8. Rebuild the project list, board, archive, and settings UI.
9. Rebuild CLI over the same persistence/workflow layer.
10. Reintroduce palettes, change-summary worker, and notification polish.

## Deliberate Simplifications Allowed
- Split `ui.rs` and `app` into many smaller files.
- Replace tui-realm with plain ratatui+crossterm if the TEA-style message/update separation is preserved.
- Make the DB async-only internally, as long as synchronous callers are given a clean boundary.
- Narrow the public crate surface if the rewrite is primarily a binary.

## Deliberate Simplifications Not Allowed
- Do not remove per-project SQLite isolation.
- Do not remove tmux bootstrap/attach workflows.
- Do not collapse tasks into “just DB rows” without status/session metadata.
- Do not make OpenCode polling synchronous/render-blocking.
- Do not remove archive/settings/project list/task palette/command palette capabilities.

## Risks To Preserve or Fix Explicitly
- Hardcoded default categories and semantics. Source: [src/db/mod.rs](/Users/Artemis/Arc/opencode-kanban/src/db/mod.rs#L1027).
- Broad public crate surface that may be accidental. Source: [src/lib.rs](/Users/Artemis/Arc/opencode-kanban/src/lib.rs#L1).
- `App` size and responsibility concentration. Source: [src/app/core.rs](/Users/Artemis/Arc/opencode-kanban/src/app/core.rs#L20).
- Mixed sync/async DB bridge complexity. Source: [src/db/mod.rs](/Users/Artemis/Arc/opencode-kanban/src/db/mod.rs#L1076).
- Fuzzy matcher panic shielding hides matcher faults but protects the UI. Source: [src/matching.rs](/Users/Artemis/Arc/opencode-kanban/src/matching.rs#L59).
