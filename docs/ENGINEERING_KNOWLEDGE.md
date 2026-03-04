# opencode-kanban — Engineering Knowledge

## Data Structures
Name: `Repo`
Kind: struct
Purpose: Persisted repository metadata.
Fields/Variants: `id`, `path`, `name`, `default_base`, `remote_url`, `created_at`, `updated_at`
Derives: `Debug`, `Clone`, `Serialize`, `Deserialize`, `Eq`, `PartialEq`
Ownership notes: All string-owned, no borrowing.
Invariants: `path` is canonicalized before insertion. Source: [src/types.rs](/Users/Artemis/Arc/opencode-kanban/src/types.rs#L6), [src/db/mod.rs](/Users/Artemis/Arc/opencode-kanban/src/db/mod.rs#L72).
Lifetime params: none
Lifecycle: Created from user path or discovered existing directory, then referenced by tasks.

Name: `Category`
Kind: struct
Purpose: Board column metadata.
Fields/Variants: `id`, `slug`, `name`, `position`, `color`, `created_at`
Derives: `Debug`, `Clone`, `Serialize`, `Deserialize`, `Eq`, `PartialEq`
Ownership notes: String-owned.
Invariants: `slug` is normalized and unique. Source: [src/types.rs](/Users/Artemis/Arc/opencode-kanban/src/types.rs#L17), [src/db/mod.rs](/Users/Artemis/Arc/opencode-kanban/src/db/mod.rs#L821).
Lifetime params: none
Lifecycle: Seeded on first run or user-created later.

Name: `Task`
Kind: struct
Purpose: Core persisted work item plus runtime/session metadata.
Fields/Variants: repo/branch/title/category/position plus tmux/OpenCode/archive/status fields.
Derives: `Debug`, `Clone`, `Serialize`, `Deserialize`, `Eq`, `PartialEq`
Ownership notes: Entirely owned; selected task reads clone frequently because UI/state code prefers value copies over borrows.
Invariants: branch non-empty on insert, unique per repo, booleans persisted as SQLite ints. Source: [src/types.rs](/Users/Artemis/Arc/opencode-kanban/src/types.rs#L27), [src/db/mod.rs](/Users/Artemis/Arc/opencode-kanban/src/db/mod.rs#L173), [src/db/mod.rs](/Users/Artemis/Arc/opencode-kanban/src/db/mod.rs#L836).
Lifetime params: none
Lifecycle: Created by workflow or CLI, updated by UI/poller, archived or deleted later.

Name: `SessionStatus`
Kind: struct
Purpose: In-memory OpenCode status snapshot.
Fields/Variants: `state`, `source`, `fetched_at`, `error`
Derives: `Debug`, `Clone`, `Serialize`, `Deserialize`, `Eq`, `PartialEq`
Ownership notes: Simple owned record.
Invariants: `source` is either `Server` or `None`; `error` is structured if present. Source: [src/types.rs](/Users/Artemis/Arc/opencode-kanban/src/types.rs#L110).
Lifetime params: none
Lifecycle: Produced by poller/provider, partially flattened into task DB metadata.

Name: `App`
Kind: struct
Purpose: Entire interactive application state.
Fields/Variants: board data, project list, dialog state, render state, caches, workers, settings, search, polling controls.
Derives: not derived
Ownership notes: Owns `Database`, vectors, hashes, settings, and all ephemeral UI state.
Invariants: startup initializes DB and caches before entering event loop. Source: [src/app/core.rs](/Users/Artemis/Arc/opencode-kanban/src/app/core.rs#L20), [src/app/core.rs](/Users/Artemis/Arc/opencode-kanban/src/app/core.rs#L123).
Lifetime params: none
Lifecycle: Constructed once in `main`, lives for the whole TUI session.

Name: `Settings`
Kind: struct
Purpose: User-configurable persistent settings.
Fields/Variants: theme, default view, board alignment, poll interval, notification backend, terminal config, project order, archived projects, keybinding overrides.
Derives: `Debug`, `Clone`, `PartialEq`, `Serialize`, `Deserialize`
Ownership notes: Owned strings/vectors.
Invariants: Values are clamped/normalized by `validate`. Source: [src/settings.rs](/Users/Artemis/Arc/opencode-kanban/src/settings.rs#L29), [src/settings.rs](/Users/Artemis/Arc/opencode-kanban/src/settings.rs#L165).
Lifetime params: none
Lifecycle: Loaded during `App` construction and saved from settings/project-order actions.

Name: `Theme`, `CustomThemeConfig`
Kind: structs
Purpose: Semantic color model for rendering.
Fields/Variants: split into base, interactive, status, tile, category, dialog palettes and optional overrides.
Derives: `Theme` is `Copy`; override config is serializable.
Ownership notes: `Theme` is immediate render-ready values; config stores owned hex strings.
Invariants: invalid preset/hex values fall back to inherited defaults. Source: [src/theme.rs](/Users/Artemis/Arc/opencode-kanban/src/theme.rs#L82), [src/theme.rs](/Users/Artemis/Arc/opencode-kanban/src/theme.rs#L146), [src/theme.rs](/Users/Artemis/Arc/opencode-kanban/src/theme.rs#L243).
Lifetime params: none
Lifecycle: Settings -> validated config -> resolved `Theme`.

Name: `ServerStatusProvider`, `SessionStatusMatch`, `SessionRecord`
Kind: structs
Purpose: HTTP client plus parsed representations of OpenCode session/status responses.
Fields/Variants: config/client/init_error on provider; session IDs, parents, titles, directories, and statuses on results.
Derives: `Debug`, `Clone`, equality on result records.
Ownership notes: Provider owns async reqwest `Client`.
Invariants: HTTP non-200 and parse failures become `SessionStatusError`. Source: [src/opencode/status_server.rs](/Users/Artemis/Arc/opencode-kanban/src/opencode/status_server.rs#L16), [src/opencode/status_server.rs](/Users/Artemis/Arc/opencode-kanban/src/opencode/status_server.rs#L23).
Lifetime params: none
Lifecycle: Provider is recreated by poller startup; result records are ephemeral.

Name: `OpenCodeServerManager`
Kind: struct
Purpose: Shared readiness state for asynchronous server bootstrapping.
Fields/Variants: `state: Arc<Mutex<OpenCodeServerState>>`
Derives: `Debug`, `Clone`, `Default`
Ownership notes: Shared mutable state specifically for startup/bootstrap result.
Invariants: state lock poisoning is tolerated by taking inner value. Source: [src/opencode/server.rs](/Users/Artemis/Arc/opencode-kanban/src/opencode/server.rs#L16).
Lifetime params: none
Lifecycle: Created during `App` startup and retained in `App` as `_server_manager`.

Name: dialog/view/search enums
Kind: enums + small structs
Purpose: Encode all finite UI modes and dialog form states.
Fields/Variants: `View`, `ViewMode`, `ActiveDialog`, `TaskSearchMode`, `TodoVisualizationMode`, `SettingsSection`, plus form-state structs.
Derives: mostly `Debug`, `Clone`, `Eq`, `PartialEq`
Ownership notes: Fully owned; no lifetimes.
Invariants: Selection indexes are maintained by update logic. Source: [src/app/state.rs](/Users/Artemis/Arc/opencode-kanban/src/app/state.rs#L255), [src/app/state.rs](/Users/Artemis/Arc/opencode-kanban/src/app/state.rs#L399).
Lifetime params: none
Lifecycle: Stored in `App` and replaced as users navigate.

## Function Contracts
Signature: `App::new_with_theme(project_name: Option<&str>, cli_theme_override: Option<ThemePreset>) -> Result<Self>`
Purpose: Build the full application state graph and background services.
Trait bounds: none
Requires: accessible DB path, tmux/OpenCode environment may be partially available.
Ensures: settings loaded, project list loaded, startup recovery run, poller started.
Behavior: Opens DB, starts server bootstrap, initializes caches, resolves theme, refreshes data/projects, optionally switches into requested project, starts poller.
Panics: not intended.
Gotchas: Construction does real I/O and background task startup; this is not a lightweight pure constructor. Source: [src/app/core.rs](/Users/Artemis/Arc/opencode-kanban/src/app/core.rs#L123).

Signature: `create_task_pipeline_with_runtime(...) -> Result<CreateTaskOutcome>`
Purpose: Create a task and all required runtime artifacts.
Trait bounds: runtime implements `CreateTaskRuntime`
Requires: valid repo selection or existing directory.
Ensures: on success, task row, tmux session, and optionally worktree exist; on failure, partial artifacts are rolled back.
Behavior: Validates input, canonicalizes paths, chooses branch/base, fetches/checks git state, creates worktree, creates tmux session, writes DB state, records repo-selection usage.
Panics: not intended.
Gotchas: Fetch errors are downgraded to warnings, not hard failures. Source: [src/app/workflows/create_task.rs](/Users/Artemis/Arc/opencode-kanban/src/app/workflows/create_task.rs#L39).

Signature: `attach_task_with_runtime(...) -> Result<AttachTaskResult>`
Purpose: Attach to a task session in the current tmux client.
Trait bounds: runtime implements `RecoveryRuntime`
Requires: repo/worktree/session may be partially missing.
Ensures: either attaches, reports missing worktree, or reports repo unavailable.
Behavior: Reuses session if it exists, recreates it if only worktree exists, switches client, may show popup, clears `needs_inspection`.
Panics: not intended.
Gotchas: A task can exist without enough runtime metadata to attach. Source: [src/app/workflows/attach.rs](/Users/Artemis/Arc/opencode-kanban/src/app/workflows/attach.rs#L16).

Signature: `spawn_status_poller(...) -> JoinHandle<()>`
Purpose: Run continuous background status synchronization.
Trait bounds: none
Requires: Tokio runtime.
Ensures: returns immediately with a task handle; loop exits when `stop` is set.
Behavior: polls DB, OpenCode endpoints, updates task metadata, refreshes caches, sends notifications.
Panics: not intended.
Gotchas: It clones cache contents each poll cycle and re-writes them wholesale. Source: [src/app/polling.rs](/Users/Artemis/Arc/opencode-kanban/src/app/polling.rs#L23).

Signature: `Database::open_async/open`
Purpose: Open or create a project database, run migrations, and seed default categories.
Trait bounds: generic `AsRef<Path>` input.
Requires: path writable if file-based.
Ensures: schema exists and defaults are seeded.
Behavior: creates parent dirs, configures SQLite, opens pool, migrates, seeds categories.
Panics: not intended.
Gotchas: `open` may block through the global runtime bridge. Source: [src/db/mod.rs](/Users/Artemis/Arc/opencode-kanban/src/db/mod.rs#L31), [src/db/mod.rs](/Users/Artemis/Arc/opencode-kanban/src/db/mod.rs#L64).

Signature: `git_change_summary_against_nearest_ancestor(repo_path: &Path) -> Result<GitChangeSummary>`
Purpose: Produce a compact summary of changes in the current branch/worktree.
Trait bounds: none
Requires: valid git repo.
Ensures: returns ancestor ref plus commit/file/change counts and top changed files.
Behavior: resolves base ref, merge-base, ahead count, shortstat, and changed file list.
Panics: not intended.
Gotchas: Summary is relative to the nearest ancestor ref, not necessarily the repo default branch. Source: [src/git/mod.rs](/Users/Artemis/Arc/opencode-kanban/src/git/mod.rs#L215).

Signature: `ensure_server_ready() -> OpenCodeServerManager`
Purpose: Guarantee the local OpenCode server is healthy or attempt to spawn it.
Trait bounds: none
Requires: `opencode` binary if no server is already healthy.
Ensures: manager state eventually becomes ready or failed.
Behavior: health-check first, otherwise spawn server and retry with exponential backoff.
Panics: not intended.
Gotchas: Startup work is asynchronous when a Tokio handle is present, synchronous otherwise. Source: [src/opencode/server.rs](/Users/Artemis/Arc/opencode-kanban/src/opencode/server.rs#L66).

Signature: `ServerStatusProvider::{list_all_session_records, fetch_status_matches, fetch_session_todo, fetch_session_messages}`
Purpose: Convert OpenCode HTTP responses into typed Rust structures.
Trait bounds: async methods, no custom traits.
Requires: reachable server.
Ensures: returns typed values or structured `SessionStatusError`.
Behavior: builds URLs, performs HTTP GET, maps auth/http/read errors, parses permissive JSON contracts.
Panics: not intended.
Gotchas: The parser is intentionally lenient because OpenCode response shapes vary. Source: [src/opencode/status_server.rs](/Users/Artemis/Arc/opencode-kanban/src/opencode/status_server.rs#L123), [src/opencode/status_server.rs](/Users/Artemis/Arc/opencode-kanban/src/opencode/status_server.rs#L179), [src/opencode/status_server.rs](/Users/Artemis/Arc/opencode-kanban/src/opencode/status_server.rs#L223), [src/opencode/status_server.rs](/Users/Artemis/Arc/opencode-kanban/src/opencode/status_server.rs#L260).

## Trait Architecture
Trait: `RecoveryRuntime`
Purpose: Abstract repo/session existence checks plus attach/open/switch actions.
Implementors: `RealRecoveryRuntime`
Key methods: `repo_exists`, `worktree_exists`, `session_exists`, `create_session`, `switch_client`, `show_attach_popup`, `open_in_new_terminal`
Design intent: make attach/recovery logic testable independently from tmux/git. Source: [src/app/runtime.rs](/Users/Artemis/Arc/opencode-kanban/src/app/runtime.rs#L16).

Trait: `CreateTaskRuntime`
Purpose: Abstract git and tmux operations needed during task creation.
Implementors: `RealCreateTaskRuntime`
Key methods: git repo/root/branch checks, worktree creation/removal, tmux create/kill/session existence
Design intent: isolate shell side effects from pipeline logic. Source: [src/app/runtime.rs](/Users/Artemis/Arc/opencode-kanban/src/app/runtime.rs#L87).

Trait: `StatusProvider`
Purpose: Minimal sync abstraction for session status lookup.
Implementors: test doubles in `opencode::tests`
Key methods: `get_status`, default `list_statuses`
Design intent: lightweight hook for status classification tests, separate from the async HTTP provider. Source: [src/opencode/mod.rs](/Users/Artemis/Arc/opencode-kanban/src/opencode/mod.rs#L56).

`dyn Trait` vs `impl Trait`
- `impl Trait` is used for runtime injection in workflows and server bootstrap helpers, favoring static dispatch. Sources: [src/app/workflows/create_task.rs](/Users/Artemis/Arc/opencode-kanban/src/app/workflows/create_task.rs#L45), [src/app/workflows/attach.rs](/Users/Artemis/Arc/opencode-kanban/src/app/workflows/attach.rs#L23), [src/opencode/server.rs](/Users/Artemis/Arc/opencode-kanban/src/opencode/server.rs#L142).
- `Box<dyn std::error::Error>` appears only in logging helpers, not core domain APIs. Source: [src/logging.rs](/Users/Artemis/Arc/opencode-kanban/src/logging.rs#L9).
- `Box<dyn Fn(&str) -> bool + Send + Sync>` appears only in tests, not production APIs. Source: [src/app/runtime.rs](/Users/Artemis/Arc/opencode-kanban/src/app/runtime.rs#L362).

Manual standard trait impls
- `Display` for `KeyBinding` to render help/keybinding strings. Source: [src/keybindings.rs](/Users/Artemis/Arc/opencode-kanban/src/keybindings.rs#L84).
- `FromStr` for `ThemePreset`, `NotificationBackend`, `TodoVisualizationMode`. Sources: [src/theme.rs](/Users/Artemis/Arc/opencode-kanban/src/theme.rs#L67), [src/notification/mod.rs](/Users/Artemis/Arc/opencode-kanban/src/notification/mod.rs#L59), [src/app/state.rs](/Users/Artemis/Arc/opencode-kanban/src/app/state.rs#L387).

## Error Handling
Strategy
- `anyhow::Result` dominates internal APIs for context-rich propagation.
- CLI adapts internal errors into `CliError { exit_code, code, message, details }`.
- OpenCode HTTP parsing uses structured `SessionStatusError { code, message }`.
Sources: [src/db/mod.rs](/Users/Artemis/Arc/opencode-kanban/src/db/mod.rs#L13), [src/cli/mod.rs](/Users/Artemis/Arc/opencode-kanban/src/cli/mod.rs#L177), [src/types.rs](/Users/Artemis/Arc/opencode-kanban/src/types.rs#L104).

Top-level panic policy
- `main` installs a panic hook that restores the terminal and prints the log path.
- Normal operations attempt graceful `Result` propagation instead of panicking.
Sources: [src/main.rs](/Users/Artemis/Arc/opencode-kanban/src/main.rs#L72), [src/main.rs](/Users/Artemis/Arc/opencode-kanban/src/main.rs#L231).

`unwrap` / `expect` inventory
- Production `unwrap`/`expect` usage in non-test code: not found in the main runtime paths reviewed.
- Test-only `expect`/`unwrap` usage is extensive across git/opencode/db/ui/tests and mostly serves fixture/assertion setup. Representative samples: [src/git/mod.rs](/Users/Artemis/Arc/opencode-kanban/src/git/mod.rs#L459), [src/opencode/status_server.rs](/Users/Artemis/Arc/opencode-kanban/src/opencode/status_server.rs#L905), [src/tmux/mod.rs](/Users/Artemis/Arc/opencode-kanban/src/tmux/mod.rs#L910), [tests/integration.rs](/Users/Artemis/Arc/opencode-kanban/tests/integration.rs#L38).
- Justification: assertions and test fixtures assume local setup succeeded; these are not relied on for production behavior.

## Concurrency & Async
Async runtime
- `tokio` is the application runtime via `#[tokio::main]`. Source: [src/main.rs](/Users/Artemis/Arc/opencode-kanban/src/main.rs#L70).

Async functions and awaits
- DB layer provides async CRUD and migrations. Source: [src/db/mod.rs](/Users/Artemis/Arc/opencode-kanban/src/db/mod.rs#L31).
- OpenCode status provider uses async reqwest methods. Source: [src/opencode/status_server.rs](/Users/Artemis/Arc/opencode-kanban/src/opencode/status_server.rs#L123).
- Status poller is a long-running `tokio::spawn` task. Source: [src/app/polling.rs](/Users/Artemis/Arc/opencode-kanban/src/app/polling.rs#L37).
- OpenCode server bootstrap uses `spawn_blocking` when a runtime exists. Source: [src/opencode/server.rs](/Users/Artemis/Arc/opencode-kanban/src/opencode/server.rs#L74).

`Arc<Mutex<_>>` inventory and justification
- `SharedApp = Arc<Mutex<App>>`: required because tui-realm component, event application, and change-summary port share a single mutable `App`. Source: [src/realm.rs](/Users/Artemis/Arc/opencode-kanban/src/realm.rs#L28).
- `OpenCodeServerManager.state: Arc<Mutex<OpenCodeServerState>>`: shared readiness state between bootstrap task and app readers. Source: [src/opencode/server.rs](/Users/Artemis/Arc/opencode-kanban/src/opencode/server.rs#L17).
- `App` session caches use `Arc<Mutex<HashMap<...>>>` for TODOs, subagent summaries, titles, and messages: poller writes them, UI reads them. Sources: [src/app/core.rs](/Users/Artemis/Arc/opencode-kanban/src/app/core.rs#L74), [src/app/polling.rs](/Users/Artemis/Arc/opencode-kanban/src/app/polling.rs#L28).
- Test locks/queues also use `Arc<Mutex<_>>` or `LazyLock<Mutex<_>>` for deterministic environment mutation and mock server sequencing. Sources: [tests/integration.rs](/Users/Artemis/Arc/opencode-kanban/tests/integration.rs#L29), [src/opencode/mod.rs](/Users/Artemis/Arc/opencode-kanban/src/opencode/mod.rs#L290).

Std-thread concurrency
- Git change summaries run in a dedicated std thread with mpsc channels, not in tokio. Source: [src/app/mod.rs](/Users/Artemis/Arc/opencode-kanban/src/app/mod.rs#L118).

Unsafe blocks
- Production unsafe blocks: not found.
- Test-only unsafe blocks exist around `env::set_var` / `env::remove_var` under Rust 2024 rules for process-global environment mutation. Sources: [src/opencode/mod.rs](/Users/Artemis/Arc/opencode-kanban/src/opencode/mod.rs#L579), [src/opencode/mod.rs](/Users/Artemis/Arc/opencode-kanban/src/opencode/mod.rs#L596), [tests/integration.rs](/Users/Artemis/Arc/opencode-kanban/tests/integration.rs#L586).
- Justification: serialized test env mutation guarded by a mutex.

## Rust Patterns
Ownership & borrowing
- State-heavy code often prefers cloning selected tasks/rows over borrowing through nested UI logic, trading allocations for simpler code paths. Evidence: `selected_task()` returns `Option<Task>`, not a reference. Source: [src/app/navigation.rs](/Users/Artemis/Arc/opencode-kanban/src/app/navigation.rs#L123).
- `Theme` is `Copy`, which makes render-time palette access cheap and borrow-free. Source: [src/theme.rs](/Users/Artemis/Arc/opencode-kanban/src/theme.rs#L82).

Error & option idioms
- Frequent use of `with_context` to preserve command/path information. Sources: [src/app/workflows/create_task.rs](/Users/Artemis/Arc/opencode-kanban/src/app/workflows/create_task.rs#L76), [src/opencode/mod.rs](/Users/Artemis/Arc/opencode-kanban/src/opencode/mod.rs#L88).
- Extensive use of early-return `let Some(...) = ... else { ... };` in attach/recovery and parsing code. Sources: [src/app/workflows/attach.rs](/Users/Artemis/Arc/opencode-kanban/src/app/workflows/attach.rs#L108), [src/opencode/mod.rs](/Users/Artemis/Arc/opencode-kanban/src/opencode/mod.rs#L45).

Macro usage
Macro: `#[tokio::main]`
Source: Tokio
Purpose: async main runtime
Non-obvious: ties the whole binary to Tokio startup. Source: [src/main.rs](/Users/Artemis/Arc/opencode-kanban/src/main.rs#L70).

Macro: `execute!`
Source: crossterm
Purpose: restore terminal state
Non-obvious: used in panic/drop recovery, not normal rendering. Source: [src/main.rs](/Users/Artemis/Arc/opencode-kanban/src/main.rs#L252).

Macro: `json!`
Source: serde_json
Purpose: build CLI JSON payloads
Non-obvious: keeps CLI schema assembly inline instead of introducing dedicated DTO types. Source: [src/cli/mod.rs](/Users/Artemis/Arc/opencode-kanban/src/cli/mod.rs#L236).

Macro: derive macros (`Serialize`, `Deserialize`, `Parser`, `Subcommand`, `Args`)
Source: serde, clap
Purpose: persistence and CLI schema declaration
Non-obvious: the app depends heavily on declarative config/CLI parsing rather than hand-rolled parsers. Sources: [src/main.rs](/Users/Artemis/Arc/opencode-kanban/src/main.rs#L35), [src/cli/mod.rs](/Users/Artemis/Arc/opencode-kanban/src/cli/mod.rs#L27), [src/types.rs](/Users/Artemis/Arc/opencode-kanban/src/types.rs#L6).

Iterator-heavy areas
- command/task palette ranking pipelines
- session record/status map construction in poller
- project discovery from directory scan
Sources: [src/command_palette.rs](/Users/Artemis/Arc/opencode-kanban/src/command_palette.rs#L75), [src/task_palette.rs](/Users/Artemis/Arc/opencode-kanban/src/task_palette.rs#L130), [src/app/polling.rs](/Users/Artemis/Arc/opencode-kanban/src/app/polling.rs#L164), [src/projects.rs](/Users/Artemis/Arc/opencode-kanban/src/projects.rs#L76).

Advanced lifetime patterns
- not found.

Build system & features
- No Cargo feature flags.
- Single crate, edition 2024.
- Build script provides dynamic version string injection.
Sources: [Cargo.toml](/Users/Artemis/Arc/opencode-kanban/Cargo.toml#L1), [build.rs](/Users/Artemis/Arc/opencode-kanban/build.rs#L1).

Testing strategy
- Many in-module unit tests across most subsystems.
- Async integration tests cover git/tmux/OpenCode lifecycle and server-failure scenarios.
- Current local sandbox result: `cargo test --lib` ran 381 tests; 355 passed, 26 failed due to environment constraints around GPG signing, listener binding, and one tokio-reactor-dependent tmux test.
Sources: [tests/integration.rs](/Users/Artemis/Arc/opencode-kanban/tests/integration.rs#L31), [tests/integration.rs](/Users/Artemis/Arc/opencode-kanban/tests/integration.rs#L109).
