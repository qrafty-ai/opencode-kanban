# opencode-kanban — Design Knowledge

## Architecture
Pattern
- TEA-style state machine around a single mutable `App` model. Events become `Message`s, messages update `App`, and `ui::render` draws from `App`. Sources: [src/app/messages.rs](/Users/Artemis/Arc/opencode-kanban/src/app/messages.rs#L12), [src/realm.rs](/Users/Artemis/Arc/opencode-kanban/src/realm.rs#L40), [src/ui.rs](/Users/Artemis/Arc/opencode-kanban/src/ui.rs#L45).

Top-level flow
1. Parse CLI args and initialize logging.
2. If a CLI subcommand exists, run CLI and exit.
3. Otherwise validate OS/tmux state and auto-bootstrap into tmux if needed.
4. Construct `App`, mount the tui-realm root component, and enter the event loop.
5. `App` startup also opens the project DB, starts OpenCode server bootstrapping, seeds settings/caches, reconciles startup task state, and spawns the status poller.
Sources: [src/main.rs](/Users/Artemis/Arc/opencode-kanban/src/main.rs#L70), [src/main.rs](/Users/Artemis/Arc/opencode-kanban/src/main.rs#L102), [src/app/core.rs](/Users/Artemis/Arc/opencode-kanban/src/app/core.rs#L123).

Module map
- `main.rs`: binary entry, runtime validation, terminal lifecycle. Source: [src/main.rs](/Users/Artemis/Arc/opencode-kanban/src/main.rs#L35).
- `lib.rs`: exports all top-level modules. Source: [src/lib.rs](/Users/Artemis/Arc/opencode-kanban/src/lib.rs#L1).
- `app/`: central domain state, TEA update/input logic, workflows, startup reconciliation, background polling. Sources: [src/app/mod.rs](/Users/Artemis/Arc/opencode-kanban/src/app/mod.rs#L1), [src/app/core.rs](/Users/Artemis/Arc/opencode-kanban/src/app/core.rs#L20).
- `ui.rs`: all ratatui/tuirealm rendering. Source: [src/ui.rs](/Users/Artemis/Arc/opencode-kanban/src/ui.rs#L45).
- `realm.rs`: adapter between tui-realm events and app messages. Source: [src/realm.rs](/Users/Artemis/Arc/opencode-kanban/src/realm.rs#L40).
- `db/mod.rs`: SQLite schema, migrations, CRUD, sync/async bridge. Source: [src/db/mod.rs](/Users/Artemis/Arc/opencode-kanban/src/db/mod.rs#L31).
- `git/mod.rs`: pure shell wrappers for git metadata/worktree operations. Source: [src/git/mod.rs](/Users/Artemis/Arc/opencode-kanban/src/git/mod.rs#L22).
- `tmux/mod.rs`: pure shell wrappers for tmux session, popup, message, and terminal-launch operations. Source: [src/tmux/mod.rs](/Users/Artemis/Arc/opencode-kanban/src/tmux/mod.rs#L34).
- `opencode/mod.rs`, `opencode/server.rs`, `opencode/status_server.rs`: command helpers, server bootstrap, HTTP polling contract/parser. Sources: [src/opencode/mod.rs](/Users/Artemis/Arc/opencode-kanban/src/opencode/mod.rs#L16), [src/opencode/server.rs](/Users/Artemis/Arc/opencode-kanban/src/opencode/server.rs#L66), [src/opencode/status_server.rs](/Users/Artemis/Arc/opencode-kanban/src/opencode/status_server.rs#L70).
- `projects.rs`: project DB file discovery and lifecycle. Source: [src/projects.rs](/Users/Artemis/Arc/opencode-kanban/src/projects.rs#L16).
- `settings.rs`, `theme.rs`, `keybindings.rs`: user customization layer. Sources: [src/settings.rs](/Users/Artemis/Arc/opencode-kanban/src/settings.rs#L29), [src/theme.rs](/Users/Artemis/Arc/opencode-kanban/src/theme.rs#L82), [src/keybindings.rs](/Users/Artemis/Arc/opencode-kanban/src/keybindings.rs#L131).
- `command_palette.rs`, `task_palette.rs`, `matching.rs`: fuzzy-search subsystems. Sources: [src/command_palette.rs](/Users/Artemis/Arc/opencode-kanban/src/command_palette.rs#L12), [src/task_palette.rs](/Users/Artemis/Arc/opencode-kanban/src/task_palette.rs#L11), [src/matching.rs](/Users/Artemis/Arc/opencode-kanban/src/matching.rs#L5).
- `notification/mod.rs`: completion notifications. Source: [src/notification/mod.rs](/Users/Artemis/Arc/opencode-kanban/src/notification/mod.rs#L8).
- `types.rs`: shared persisted/runtime DTOs. Source: [src/types.rs](/Users/Artemis/Arc/opencode-kanban/src/types.rs#L6).

End-to-end data flow
1. User selects or creates a project, which resolves to a SQLite file path.
2. `Database` loads repos/categories/tasks and `App` caches them in memory.
3. UI actions emit `Message`s.
4. Update/workflow code mutates the DB and/or invokes runtime traits for git/tmux.
5. Background poller independently mutates task status metadata and session caches based on OpenCode HTTP responses.
6. Render reads the in-memory `App` plus transient caches and paints the current view.
Sources: [src/app/core.rs](/Users/Artemis/Arc/opencode-kanban/src/app/core.rs#L394), [src/app/workflows/create_task.rs](/Users/Artemis/Arc/opencode-kanban/src/app/workflows/create_task.rs#L39), [src/app/polling.rs](/Users/Artemis/Arc/opencode-kanban/src/app/polling.rs#L23), [src/ui.rs](/Users/Artemis/Arc/opencode-kanban/src/ui.rs#L45).

Pub vs private boundaries
- Public crate surface is broad and library-like: most subsystems are exported directly from `lib.rs`. Source: [src/lib.rs](/Users/Artemis/Arc/opencode-kanban/src/lib.rs#L1).
- Within `app`, core internals are private modules (`core`, `input`, `navigation`, `side_panel`, `update`, `log`) while state/messages/dialogs/workflows/runtime are public or re-exported. Source: [src/app/mod.rs](/Users/Artemis/Arc/opencode-kanban/src/app/mod.rs#L1).
- Runtime traits are public so tests and other callers can substitute implementations. Source: [src/app/runtime.rs](/Users/Artemis/Arc/opencode-kanban/src/app/runtime.rs#L16), [src/app/runtime.rs](/Users/Artemis/Arc/opencode-kanban/src/app/runtime.rs#L87).

Feature flags
- not found in `Cargo.toml`.

Build scripts / proc macros
- `build.rs` injects `OPENCODE_KANBAN_BUILD_VERSION` from env override or `git describe`.
- not found: proc-macro crates or generated source.
Source: [build.rs](/Users/Artemis/Arc/opencode-kanban/build.rs#L1).

## Public API Surface
Main public entry points
- `app::App`: primary state container and startup constructor. Source: [src/app/core.rs](/Users/Artemis/Arc/opencode-kanban/src/app/core.rs#L20).
- `cli::run`: project-scoped CLI dispatcher. Source: [src/cli/mod.rs](/Users/Artemis/Arc/opencode-kanban/src/cli/mod.rs#L157).
- `Database`: async/sync CRUD facade for repos/categories/tasks and command frequencies. Source: [src/db/mod.rs](/Users/Artemis/Arc/opencode-kanban/src/db/mod.rs#L26).
- `git::*`: shell-backed repo/worktree operations. Source: [src/git/mod.rs](/Users/Artemis/Arc/opencode-kanban/src/git/mod.rs#L22).
- `tmux::*`: shell-backed tmux and terminal-launch operations. Source: [src/tmux/mod.rs](/Users/Artemis/Arc/opencode-kanban/src/tmux/mod.rs#L34).
- `opencode::*`: binding-state classification, launch/resume, attach command derivation, directory-to-session lookup. Source: [src/opencode/mod.rs](/Users/Artemis/Arc/opencode-kanban/src/opencode/mod.rs#L32).
- `opencode::ServerStatusProvider`: OpenCode HTTP client and parser. Source: [src/opencode/status_server.rs](/Users/Artemis/Arc/opencode-kanban/src/opencode/status_server.rs#L16).
- `projects::*`: filesystem lifecycle for project DBs. Source: [src/projects.rs](/Users/Artemis/Arc/opencode-kanban/src/projects.rs#L16).
- `settings::Settings`, `theme::ThemePreset`, `theme::Theme`, `keybindings::Keybindings`: customization APIs. Sources: [src/settings.rs](/Users/Artemis/Arc/opencode-kanban/src/settings.rs#L29), [src/theme.rs](/Users/Artemis/Arc/opencode-kanban/src/theme.rs#L7), [src/keybindings.rs](/Users/Artemis/Arc/opencode-kanban/src/keybindings.rs#L131).
- `command_palette::*`, `task_palette::*`, `matching::*`: reusable palette/ranking APIs. Sources: [src/command_palette.rs](/Users/Artemis/Arc/opencode-kanban/src/command_palette.rs#L12), [src/task_palette.rs](/Users/Artemis/Arc/opencode-kanban/src/task_palette.rs#L11), [src/matching.rs](/Users/Artemis/Arc/opencode-kanban/src/matching.rs#L5).

⚠️ Public-surface observation
- The crate exports far more internals than a minimal binary crate needs. This is useful for tests/integration reuse, but a rewrite should decide whether it is truly a library+binaries project or just a binary with broad accidental visibility. Evidence: [src/lib.rs](/Users/Artemis/Arc/opencode-kanban/src/lib.rs#L1).

## Design Decision Log
Decision: Single stateful `App` object owns almost everything.
Alternatives: Split state per screen/subsystem or use multiple stores/services.
Reason: Simpler TEA update loop and easier rendering from one model.
Impact: High cohesion for TUI work, but `App` is a large responsibility bucket. Evidence: [src/app/core.rs](/Users/Artemis/Arc/opencode-kanban/src/app/core.rs#L20).

Decision: Runtime shelling is abstracted behind traits.
Alternatives: Call git/tmux directly from workflows.
Reason: Enables deterministic tests around task creation/attach/recovery without shelling out.
Impact: Good seam for rewrite; workflows are the portable domain layer. Evidence: [src/app/runtime.rs](/Users/Artemis/Arc/opencode-kanban/src/app/runtime.rs#L16), [src/app/runtime.rs](/Users/Artemis/Arc/opencode-kanban/src/app/runtime.rs#L87).

Decision: Sync and async DB APIs coexist.
Alternatives: Async-only DB layer or blocking-only DB layer.
Reason: TUI update code is mostly synchronous, but background polling is async.
Impact: Requires `block_on_db` and a global runtime bridge. Evidence: [src/db/mod.rs](/Users/Artemis/Arc/opencode-kanban/src/db/mod.rs#L64), [src/db/mod.rs](/Users/Artemis/Arc/opencode-kanban/src/db/mod.rs#L1076).

Decision: OpenCode status is modeled as eventually-consistent background metadata.
Alternatives: Query status synchronously during rendering/selection.
Reason: Avoid blocking the UI and support TODO/message caches.
Impact: Poller complexity increases, but UI remains responsive. Evidence: [src/app/polling.rs](/Users/Artemis/Arc/opencode-kanban/src/app/polling.rs#L23).

Decision: Project identity is the SQLite filename.
Alternatives: Separate project metadata registry.
Reason: Minimal filesystem model, simple discovery.
Impact: Renaming a project is just renaming the DB file. Evidence: [src/projects.rs](/Users/Artemis/Arc/opencode-kanban/src/projects.rs#L22), [src/projects.rs](/Users/Artemis/Arc/opencode-kanban/src/projects.rs#L110).

Decision: tmux socket selection is environment-driven.
Alternatives: Hardcode a socket or use default tmux server always.
Reason: Test isolation and optional separation from the user’s default tmux server.
Impact: Cleaner tests and safer session scoping. Evidence: [src/tmux/mod.rs](/Users/Artemis/Arc/opencode-kanban/src/tmux/mod.rs#L335), [src/tmux/mod.rs](/Users/Artemis/Arc/opencode-kanban/src/tmux/mod.rs#L591).

Decision: OpenCode server bootstrap occurs during `App` construction.
Alternatives: Lazy-start on first attach/poll request.
Reason: Ensure polling and attach commands have a server available early.
Impact: Startup does more work and can fail into a stored status. Evidence: [src/app/core.rs](/Users/Artemis/Arc/opencode-kanban/src/app/core.rs#L127), [src/opencode/server.rs](/Users/Artemis/Arc/opencode-kanban/src/opencode/server.rs#L66).

Decision: Palette ranking uses nucleo plus a panic-catching shim.
Alternatives: Trust matcher, use simpler substring matching, or swap matcher libraries.
Reason: Better fuzzy UX while containing matcher instability.
Impact: ⚠️ A caught panic can hide underlying matcher problems, but protects the app from crashing on malformed input. Evidence: [src/matching.rs](/Users/Artemis/Arc/opencode-kanban/src/matching.rs#L59), [src/command_palette.rs](/Users/Artemis/Arc/opencode-kanban/src/command_palette.rs#L102).

Decision: Default categories are seeded with fixed names and order.
Alternatives: Require explicit initial setup or configurable board templates.
Reason: Zero-config first-run experience.
Impact: ⚠️ Hardcoded board semantics leak into tests and workflows. Evidence: [src/db/mod.rs](/Users/Artemis/Arc/opencode-kanban/src/db/mod.rs#L1021).

Comment inventory
- TODO comments: not found.
- FIXME comments: not found.
- HACK comments: not found.
- NOTE comments: not found.
- Commented-out code suggesting abandoned approaches: not found.
