# opencode-kanban — Functionality Inventory

## Purpose
`opencode-kanban` is a terminal-first kanban board for managing software tasks as Git worktrees plus tmux/OpenCode sessions. The project is explicitly positioned as an OpenCode-centric workflow manager with stable session-state detection, TODO tracking, and subagent TODO summaries layered on top of a board UI and a small project-scoped CLI. Sources: [README.md](/Users/Artemis/Arc/opencode-kanban/README.md#L8), [README.md](/Users/Artemis/Arc/opencode-kanban/README.md#L13), [src/main.rs](/Users/Artemis/Arc/opencode-kanban/src/main.rs#L35).

## Feature & Capability List
Feature: TUI application boot
Description: Starts the terminal UI, initializes logging, restores the terminal on panic/drop, and drives the tui-realm message loop.
Entry point: `main`, `run_app`
Status: fully implemented
Notes: Requires Linux or macOS and tmux availability. Sources: [src/main.rs](/Users/Artemis/Arc/opencode-kanban/src/main.rs#L70), [src/main.rs](/Users/Artemis/Arc/opencode-kanban/src/main.rs#L102), [src/main.rs](/Users/Artemis/Arc/opencode-kanban/src/main.rs#L167).

Feature: Auto-bootstrap into tmux
Description: If launched outside tmux, auto-attaches to an existing `opencode-kanban` session or creates one and re-executes inside it.
Entry point: `validate_runtime_environment`
Status: fully implemented
Notes: This is the default app startup path, not a separate command. Sources: [README.md](/Users/Artemis/Arc/opencode-kanban/README.md#L49), [src/main.rs](/Users/Artemis/Arc/opencode-kanban/src/main.rs#L167).

Feature: Project-scoped board databases
Description: Uses one SQLite file per project and supports listing, creating, renaming, deleting, ordering, and archiving projects.
Entry point: `projects::*`, project-list UI
Status: fully implemented
Notes: Project files live under the local data dir and are discovered by `.sqlite` extension. Sources: [README.md](/Users/Artemis/Arc/opencode-kanban/README.md#L98), [src/projects.rs](/Users/Artemis/Arc/opencode-kanban/src/projects.rs#L16), [src/projects.rs](/Users/Artemis/Arc/opencode-kanban/src/projects.rs#L67), [src/ui.rs](/Users/Artemis/Arc/opencode-kanban/src/ui.rs#L66).

Feature: Board rendering
Description: Renders either a kanban column layout or a side-panel detail layout, plus archive view, settings view, dialogs, search overlays, and context menus.
Entry point: `ui::render`
Status: fully implemented
Notes: The project list is implemented as a first-class top-level view. Sources: [src/ui.rs](/Users/Artemis/Arc/opencode-kanban/src/ui.rs#L45), [src/app/state.rs](/Users/Artemis/Arc/opencode-kanban/src/app/state.rs#L255).

Feature: Repo persistence and discovery
Description: Persists repos with canonical path, display name, detected default base branch, and remote URL.
Entry point: `Database::add_repo[_async]`
Status: fully implemented
Notes: Repo metadata is inferred from git commands at insert time. Sources: [src/types.rs](/Users/Artemis/Arc/opencode-kanban/src/types.rs#L6), [src/db/mod.rs](/Users/Artemis/Arc/opencode-kanban/src/db/mod.rs#L72).

Feature: Category persistence and board columns
Description: Persists named/sluggified categories with ordering and optional semantic color, and seeds default columns on first run.
Entry point: `Database::add_category[_async]`, `seed_default_categories_async`
Status: fully implemented
Notes: Default categories are hardcoded as `TODO`, `IN PROGRESS`, and `DONE`. Sources: [src/types.rs](/Users/Artemis/Arc/opencode-kanban/src/types.rs#L17), [src/db/mod.rs](/Users/Artemis/Arc/opencode-kanban/src/db/mod.rs#L578), [src/db/mod.rs](/Users/Artemis/Arc/opencode-kanban/src/db/mod.rs#L1021).

Feature: Task persistence
Description: Persists tasks with board position, repo/branch/title, tmux session name, worktree path, runtime status metadata, OpenCode session binding, archive state, attach-popup state, and inspection state.
Entry point: `Database::add_task[_async]` and update methods
Status: fully implemented
Notes: `repo_id + branch` is unique, so one branch maps to one task per repo. Sources: [src/types.rs](/Users/Artemis/Arc/opencode-kanban/src/types.rs#L27), [src/db/mod.rs](/Users/Artemis/Arc/opencode-kanban/src/db/mod.rs#L166), [src/db/mod.rs](/Users/Artemis/Arc/opencode-kanban/src/db/mod.rs#L835).

Feature: New task from existing directory
Description: Accepts an already-existing git working directory, resolves repo root, reads current branch, optionally inserts the repo into the board, and creates a task bound to that directory instead of creating a new worktree.
Entry point: `create_task_pipeline_with_runtime`
Status: fully implemented
Notes: Rejects empty, missing, non-directory, non-git, and detached-HEAD inputs. Sources: [src/app/workflows/create_task.rs](/Users/Artemis/Arc/opencode-kanban/src/app/workflows/create_task.rs#L39).

Feature: New task from new worktree
Description: Selects or inserts a repo, derives/validates branch name, optionally checks base branch freshness, creates a worktree, creates a tmux session running `opencode attach`, then persists the task.
Entry point: `create_task_pipeline_with_runtime`
Status: fully implemented
Notes: Includes rollback if later steps fail. Sources: [src/app/workflows/create_task.rs](/Users/Artemis/Arc/opencode-kanban/src/app/workflows/create_task.rs#L108).

Feature: Auto-generated branch names
Description: Generates human-readable branch names like `feature/amber-rocket-123` when the branch input is blank but title is present.
Entry point: `generate_human_readable_branch_slug`
Status: fully implemented
Notes: Uses UUID bytes to select adjective/noun/suffix. Sources: [src/app/workflows/create_task.rs](/Users/Artemis/Arc/opencode-kanban/src/app/workflows/create_task.rs#L211), [src/app/workflows/create_task.rs](/Users/Artemis/Arc/opencode-kanban/src/app/workflows/create_task.rs#L223).

Feature: Task attach in current tmux client
Description: Ensures repo/worktree/session validity, recreates the session if missing, switches the tmux client into the task session, and optionally shows a helper popup with navigation and TODOs.
Entry point: `attach_task_with_runtime`
Status: fully implemented
Notes: If the repo or worktree is unavailable, returns a structured early result instead of attaching. Sources: [src/app/workflows/attach.rs](/Users/Artemis/Arc/opencode-kanban/src/app/workflows/attach.rs#L16), [src/app/workflows/attach.rs](/Users/Artemis/Arc/opencode-kanban/src/app/workflows/attach.rs#L80).

Feature: Task attach in a new terminal
Description: Opens an already-existing or newly-created tmux session in a new terminal emulator using configurable executable/args.
Entry point: `open_task_in_new_terminal_with_runtime`
Status: fully implemented
Notes: Uses `termlauncher` rather than shelling out to `open`/`osascript`. Sources: [src/app/workflows/attach.rs](/Users/Artemis/Arc/opencode-kanban/src/app/workflows/attach.rs#L47), [src/tmux/mod.rs](/Users/Artemis/Arc/opencode-kanban/src/tmux/mod.rs#L100).

Feature: tmux helper bindings
Description: Installs tmux `Prefix+K` to return to the previous client and `Prefix+O` to reopen the task helper popup.
Entry point: `tmux_switch_client`, `tmux_show_popup`
Status: fully implemented
Notes: The popup content is generated from task metadata and TODO state. Sources: [README.md](/Users/Artemis/Arc/opencode-kanban/README.md#L45), [src/tmux/mod.rs](/Users/Artemis/Arc/opencode-kanban/src/tmux/mod.rs#L85), [src/tmux/mod.rs](/Users/Artemis/Arc/opencode-kanban/src/tmux/mod.rs#L503), [src/app/workflows/attach.rs](/Users/Artemis/Arc/opencode-kanban/src/app/workflows/attach.rs#L220).

Feature: Git helpers
Description: Detects default branch, fetches origin, checks if a base branch is stale relative to origin, lists branches/tags, creates/removes worktrees, deletes branches, reads remote URL, and computes a change summary against the nearest ancestor ref.
Entry point: `git::*`
Status: fully implemented
Notes: Change summary powers the side-panel detail view. Sources: [src/git/mod.rs](/Users/Artemis/Arc/opencode-kanban/src/git/mod.rs#L22), [src/git/mod.rs](/Users/Artemis/Arc/opencode-kanban/src/git/mod.rs#L215).

Feature: Background OpenCode server bootstrap
Description: Checks local OpenCode server health and, if needed, starts `opencode serve` and waits for readiness with backoff.
Entry point: `ensure_server_ready`
Status: fully implemented
Notes: Readiness is tracked as `Starting`, `ReadyAttached`, `ReadySpawned`, or `Failed`. Sources: [src/opencode/server.rs](/Users/Artemis/Arc/opencode-kanban/src/opencode/server.rs#L8), [src/opencode/server.rs](/Users/Artemis/Arc/opencode-kanban/src/opencode/server.rs#L66), [src/opencode/server.rs](/Users/Artemis/Arc/opencode-kanban/src/opencode/server.rs#L142).

Feature: OpenCode session launch/resume and binding
Description: Can launch/resume OpenCode directly, derive the `opencode attach` command line, query server sessions by working directory, and classify task bindings as bound/stale/unbound.
Entry point: `opencode::*`
Status: fully implemented
Notes: Session ID discovery on raw `opencode` launch is regex-based and depends on command output containing a UUID. Sources: [src/opencode/mod.rs](/Users/Artemis/Arc/opencode-kanban/src/opencode/mod.rs#L32), [src/opencode/mod.rs](/Users/Artemis/Arc/opencode-kanban/src/opencode/mod.rs#L77), [src/opencode/mod.rs](/Users/Artemis/Arc/opencode-kanban/src/opencode/mod.rs#L202), [src/opencode/mod.rs](/Users/Artemis/Arc/opencode-kanban/src/opencode/mod.rs#L215).

Feature: Background status/TODO/message polling
Description: Polls OpenCode session records and status endpoints, updates task runtime status and metadata, fetches TODO lists and messages, tracks subagent TODO summaries, and marks completed tasks as needing inspection.
Entry point: `spawn_status_poller`
Status: fully implemented
Notes: Poller works even when repo availability changes and preserves prior caches if fetches fail. Sources: [src/app/polling.rs](/Users/Artemis/Arc/opencode-kanban/src/app/polling.rs#L23), [src/opencode/status_server.rs](/Users/Artemis/Arc/opencode-kanban/src/opencode/status_server.rs#L123), [src/opencode/status_server.rs](/Users/Artemis/Arc/opencode-kanban/src/opencode/status_server.rs#L179), [src/opencode/status_server.rs](/Users/Artemis/Arc/opencode-kanban/src/opencode/status_server.rs#L223), [src/opencode/status_server.rs](/Users/Artemis/Arc/opencode-kanban/src/opencode/status_server.rs#L260).

Feature: Completion notifications
Description: Emits task completion notifications through tmux, system notifications, both, or neither.
Entry point: `notify_task_completion`
Status: fully implemented
Notes: Triggered only on root-session completion transitions. Sources: [src/notification/mod.rs](/Users/Artemis/Arc/opencode-kanban/src/notification/mod.rs#L73), [src/app/polling.rs](/Users/Artemis/Arc/opencode-kanban/src/app/polling.rs#L216).

Feature: Project list and project detail summary
Description: Presents a dedicated project selection screen with metadata summary for the selected project DB.
Entry point: `render_project_list`, `load_project_detail`
Status: fully implemented
Notes: Includes task/running/repo/category counts and DB file size. Sources: [src/ui.rs](/Users/Artemis/Arc/opencode-kanban/src/ui.rs#L66), [src/app/core.rs](/Users/Artemis/Arc/opencode-kanban/src/app/core.rs#L89).

Feature: Settings UI
Description: Edits general settings, category colors, keybindings, and repo management through an in-app settings screen.
Entry point: settings view and `Settings` persistence
Status: fully implemented
Notes: General settings include theme, default view, board alignment, polling cadence, notifications, panel width, and terminal-launch config. Sources: [src/app/state.rs](/Users/Artemis/Arc/opencode-kanban/src/app/state.rs#L263), [src/settings.rs](/Users/Artemis/Arc/opencode-kanban/src/settings.rs#L29), [src/ui.rs](/Users/Artemis/Arc/opencode-kanban/src/ui.rs#L4455).

Feature: Custom keybindings
Description: Loads default keymaps and overlays user-defined bindings from settings/legacy config formats.
Entry point: `Keybindings::load`
Status: fully implemented
Notes: Keymaps are context-scoped for global, project-list, and board states. Sources: [src/keybindings.rs](/Users/Artemis/Arc/opencode-kanban/src/keybindings.rs#L10), [src/keybindings.rs](/Users/Artemis/Arc/opencode-kanban/src/keybindings.rs#L417).

Feature: Command palette
Description: Fuzzy-searchable command palette ranking commands by fuzzy score and usage recency/frequency.
Entry point: `CommandPaletteState`, `rank_commands`
Status: fully implemented
Notes: Persists usage frequencies in SQLite `command_frequency`. Sources: [src/command_palette.rs](/Users/Artemis/Arc/opencode-kanban/src/command_palette.rs#L27), [src/command_palette.rs](/Users/Artemis/Arc/opencode-kanban/src/command_palette.rs#L75), [src/db/mod.rs](/Users/Artemis/Arc/opencode-kanban/src/db/mod.rs#L760).

Feature: Global task palette
Description: Fuzzy-searches tasks across projects and jumps directly to the selected task, including cross-project switches.
Entry point: `TaskPaletteState`, `rank_task_candidates`
Status: fully implemented
Notes: Search surface is title + branch + repo + category + project. Sources: [src/task_palette.rs](/Users/Artemis/Arc/opencode-kanban/src/task_palette.rs#L59), [src/task_palette.rs](/Users/Artemis/Arc/opencode-kanban/src/task_palette.rs#L130).

Feature: Archive workflows
Description: Archives/unarchives tasks and provides a dedicated archive view for old tasks.
Entry point: DB archive methods, archive UI/messages
Status: fully implemented
Notes: Archived tasks are excluded from active board queries. Sources: [src/db/mod.rs](/Users/Artemis/Arc/opencode-kanban/src/db/mod.rs#L283), [src/db/mod.rs](/Users/Artemis/Arc/opencode-kanban/src/db/mod.rs#L304), [src/app/messages.rs](/Users/Artemis/Arc/opencode-kanban/src/app/messages.rs#L68), [src/ui.rs](/Users/Artemis/Arc/opencode-kanban/src/ui.rs#L251).

Feature: Change summary worker
Description: Computes git diff summaries against the nearest ancestor in a background std thread and feeds results back into the TUI.
Entry point: `spawn_change_summary_worker`
Status: fully implemented
Notes: This is separate from the OpenCode status poller and exists for side-panel detail performance. Sources: [src/app/mod.rs](/Users/Artemis/Arc/opencode-kanban/src/app/mod.rs#L118).

## Operational Modes
Mode: TUI
Trigger: Launch with no subcommand
Behavior differences: Initializes terminal/raw mode, creates `App`, enters tui-realm event loop, shows project list or board depending on startup state. Sources: [src/main.rs](/Users/Artemis/Arc/opencode-kanban/src/main.rs#L102).

Mode: CLI
Trigger: Launch with `task ...` or `category ...` subcommand
Behavior differences: Requires `--project`, does not enter the TUI, returns exit codes and either text or JSON output. Sources: [src/main.rs](/Users/Artemis/Arc/opencode-kanban/src/main.rs#L105), [src/cli/mod.rs](/Users/Artemis/Arc/opencode-kanban/src/cli/mod.rs#L157).

Mode: In-tmux bootstrap
Trigger: Start binary without `$TMUX`
Behavior differences: Replaces process flow with tmux attach/new-session and exits before UI setup. Sources: [src/main.rs](/Users/Artemis/Arc/opencode-kanban/src/main.rs#L174).

Mode: View modes inside board
Trigger: Toggle between `Kanban` and `SidePanel`
Behavior differences: One mode renders columns; the other renders grouped list/details/log panes. Sources: [src/app/state.rs](/Users/Artemis/Arc/opencode-kanban/src/app/state.rs#L333), [src/ui.rs](/Users/Artemis/Arc/opencode-kanban/src/ui.rs#L240).

Mode: TODO visualization mode
Trigger: Settings/env/command toggle
Behavior differences: Renders TODO information as a summary or checklist. Sources: [src/app/state.rs](/Users/Artemis/Arc/opencode-kanban/src/app/state.rs#L365), [src/app/core.rs](/Users/Artemis/Arc/opencode-kanban/src/app/core.rs#L147).

## Data & I/O
Input formats and sources
- CLI args via `clap`. Source: [src/main.rs](/Users/Artemis/Arc/opencode-kanban/src/main.rs#L35).
- Keyboard/mouse/resize/tick events via tui-realm/crossterm. Source: [src/realm.rs](/Users/Artemis/Arc/opencode-kanban/src/realm.rs#L40).
- SQLite project DB files. Source: [src/projects.rs](/Users/Artemis/Arc/opencode-kanban/src/projects.rs#L16), [src/db/mod.rs](/Users/Artemis/Arc/opencode-kanban/src/db/mod.rs#L31).
- `settings.toml` in XDG config. Source: [src/settings.rs](/Users/Artemis/Arc/opencode-kanban/src/settings.rs#L77).
- Git command output for repo metadata and worktree operations. Source: [src/git/mod.rs](/Users/Artemis/Arc/opencode-kanban/src/git/mod.rs#L22).
- tmux command output for session/process information. Source: [src/tmux/mod.rs](/Users/Artemis/Arc/opencode-kanban/src/tmux/mod.rs#L49).
- OpenCode HTTP API and command output. Source: [src/opencode/status_server.rs](/Users/Artemis/Arc/opencode-kanban/src/opencode/status_server.rs#L123), [src/opencode/mod.rs](/Users/Artemis/Arc/opencode-kanban/src/opencode/mod.rs#L77).

Output formats and destinations
- Alternate-screen TUI rendering. Source: [src/main.rs](/Users/Artemis/Arc/opencode-kanban/src/main.rs#L215), [src/ui.rs](/Users/Artemis/Arc/opencode-kanban/src/ui.rs#L45).
- JSON or text CLI output. Source: [src/main.rs](/Users/Artemis/Arc/opencode-kanban/src/main.rs#L105), [src/cli/mod.rs](/Users/Artemis/Arc/opencode-kanban/src/cli/mod.rs#L170).
- tmux sessions, popups, and display messages. Source: [src/tmux/mod.rs](/Users/Artemis/Arc/opencode-kanban/src/tmux/mod.rs#L57), [src/tmux/mod.rs](/Users/Artemis/Arc/opencode-kanban/src/tmux/mod.rs#L132), [src/tmux/mod.rs](/Users/Artemis/Arc/opencode-kanban/src/tmux/mod.rs#L219).
- Optional system notifications. Source: [src/notification/mod.rs](/Users/Artemis/Arc/opencode-kanban/src/notification/mod.rs#L131).
- Log file under app log directory. Source: [src/main.rs](/Users/Artemis/Arc/opencode-kanban/src/main.rs#L72), [src/logging.rs](/Users/Artemis/Arc/opencode-kanban/src/logging.rs#L9).

External systems / integrations
- `tmux`
- `git`
- `opencode` binary
- OpenCode local HTTP server
- OS notification service via `notify-rust`
Sources: [README.md](/Users/Artemis/Arc/opencode-kanban/README.md#L20), [src/tmux/mod.rs](/Users/Artemis/Arc/opencode-kanban/src/tmux/mod.rs#L34), [src/opencode/server.rs](/Users/Artemis/Arc/opencode-kanban/src/opencode/server.rs#L204), [src/notification/mod.rs](/Users/Artemis/Arc/opencode-kanban/src/notification/mod.rs#L131).

What persists between runs
- Project DBs: repos, categories, tasks, archive state, session binding metadata, command usage frequency. Sources: [src/db/mod.rs](/Users/Artemis/Arc/opencode-kanban/src/db/mod.rs#L805).
- Settings TOML: themes, view settings, archived projects, terminal launcher, keybindings. Sources: [src/settings.rs](/Users/Artemis/Arc/opencode-kanban/src/settings.rs#L29).
- not found: any other durable cache outside SQLite/settings.

## Implemented Algorithms
Algorithm/Logic: Default branch detection
Location: `git_detect_default_branch`
Purpose: Prefer `origin/HEAD`, then `main`, then `master`, then first non-HEAD branch.
Complexity/notes: Falls back safely even when remotes are incomplete. Source: [src/git/mod.rs](/Users/Artemis/Arc/opencode-kanban/src/git/mod.rs#L22).

Algorithm/Logic: Worktree path derivation
Location: `derive_worktree_path`
Purpose: Generate stable, collision-resistant worktree directories from repo and branch names.
Complexity/notes: Inputs are sanitized for filesystem safety. Source: [src/git/mod.rs](/Users/Artemis/Arc/opencode-kanban/src/git/mod.rs#L337).

Algorithm/Logic: Change summary against nearest ancestor
Location: `git_change_summary_against_nearest_ancestor`
Purpose: Find merge-base against a best ancestor ref, then compute commits ahead, shortstat, and top changed files.
Complexity/notes: Intended for compact side-panel inspection rather than exhaustive diff display. Source: [src/git/mod.rs](/Users/Artemis/Arc/opencode-kanban/src/git/mod.rs#L215).

Algorithm/Logic: Session-name allocation
Location: `next_available_session_name_by`
Purpose: Reuse or derive the first unused tmux session name for a repo/branch/project tuple.
Complexity/notes: Tries unsuffixed name first, then `-2` through `-9999`. Source: [src/app/runtime.rs](/Users/Artemis/Arc/opencode-kanban/src/app/runtime.rs#L249).

Algorithm/Logic: Status-match root promotion
Location: `select_status_match`, `find_eldest_ancestor`
Purpose: Promote a status match to the eldest ancestor/root session when the status endpoint returns descendant sessions.
Complexity/notes: Uses parent maps from both status and session-record endpoints. Source: [src/app/polling.rs](/Users/Artemis/Arc/opencode-kanban/src/app/polling.rs#L190), [src/app/polling.rs](/Users/Artemis/Arc/opencode-kanban/src/app/polling.rs#L425).

Algorithm/Logic: Subagent TODO summary extraction
Location: `live_subagent_session_ids`, `build_subagent_todo_summaries`
Purpose: Find running descendant sessions and reduce each TODO list to `(completed,total)`.
Complexity/notes: Ignores non-descendants and deduplicates IDs. Source: [src/app/polling.rs](/Users/Artemis/Arc/opencode-kanban/src/app/polling.rs#L330).

Algorithm/Logic: Fuzzy palette ranking with safety wrapper
Location: `rank_commands`, `rank_task_candidates`, `safe_fuzzy_indices`
Purpose: Rank command/task search results while preventing nucleo matcher panics from crashing the app.
Complexity/notes: Pre-filters via ASCII subsequence and catches matcher panics. Sources: [src/command_palette.rs](/Users/Artemis/Arc/opencode-kanban/src/command_palette.rs#L75), [src/task_palette.rs](/Users/Artemis/Arc/opencode-kanban/src/task_palette.rs#L130), [src/matching.rs](/Users/Artemis/Arc/opencode-kanban/src/matching.rs#L66).

Algorithm/Logic: Command-frequency recency boost
Location: `recency_frequency_bonus`
Purpose: Blend logarithmic frequency with exponential recency decay.
Complexity/notes: Used for command and repo ranking. Source: [src/matching.rs](/Users/Artemis/Arc/opencode-kanban/src/matching.rs#L5).

## Unimplemented / Abandoned
- `todo!()` not found.
- `unimplemented!()` not found.
- `TODO` comments not found as implementation notes.
- `FIXME` comments not found.
- `HACK` comments not found.
- `NOTE` comments not found.
- ⚠️ Hardcoded default category seed remains in behavior: `self.add_category_async("TODO", 0, None).await?;` plus fixed companion categories `IN PROGRESS` and `DONE`. Source: [src/db/mod.rs](/Users/Artemis/Arc/opencode-kanban/src/db/mod.rs#L1027).
- not found: feature flags, abandoned modules, commented-out alternative implementations, or compile-gated experimental features.
