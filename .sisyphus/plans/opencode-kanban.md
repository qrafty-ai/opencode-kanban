# OpenCode Kanban - Build Plan

## TL;DR

> **Quick Summary**: Build a Rust TUI kanban board for managing git worktrees and OpenCode sessions across multiple repositories, orchestrated via tmux sessions. Each "task" on the board corresponds to a worktree/branch with a dedicated tmux session running OpenCode.
> 
> **Deliverables**:
> - `opencode-kanban` binary: Rust TUI with horizontal kanban columns
> - SQLite-backed persistence for tasks, repos, categories, session mappings
> - Git worktree lifecycle management (create/delete via git CLI)
> - Tmux session orchestration (create/attach/detect/recover)
> - OpenCode integration (launch, resume by session ID, status detection via pane capture)
> - Crash recovery with lazy re-spawn
> 
> **Estimated Effort**: Large
> **Parallel Execution**: YES - 3 waves
> **Critical Path**: Task 1 (scaffold) → Task 2 (DB) → Task 3 (git) → Task 4 (tmux) → Tasks 5-7 (parallel UI + OpenCode + recovery) → Tasks 8-10 (polish)

---

## Context

### Original Request
A TUI program for managing multiple git repo/worktree and related OpenCode sessions in kanban style. Rust TUI, each "task" = one worktree/branch, using tmux sessions for orchestration.

### Interview Summary
**Key Discussions**:
- **Multi-repo scope**: Single kanban board manages tasks across MANY different repos. User picks repo when creating a task.
- **Kanban layout**: Classic horizontal columns (TODO | IN PROGRESS | DONE), Trello-style.
- **Worktree location**: Centralized base dir via `dirs` crate (Linux: `~/.local/share/opencode-kanban/worktrees/`, macOS: `~/Library/Application Support/opencode-kanban/worktrees/`).
- **Git**: Shell out to git CLI (simpler, uses user's credentials/SSH agent).
- **OpenCode-only**: Deep integration with session ID tracking, status detection, resume. Not generic multi-tool.
- **Persistence**: SQLite via `rusqlite`.
- **Launch model**: Kanban TUI runs in its own dedicated tmux session. Each task is a separate tmux session. Switching = `tmux switch-client`.
- **Status detection**: Tmux pane content parsing (AoE-style). Capture last 50 lines, pattern match.
- **Task lifecycle**: DONE = label only (worktree persists). Deletion has confirmation + cleanup options.
- **Crash recovery**: Detect dead sessions, show "dead" status, auto-recreate on user attach.
- **Navigation**: Both vim (hjkl) and arrow keys + **full mouse support** (click-to-select, wheel scroll).
- **Testing**: TDD approach with `cargo test`.
- **Rust edition**: 2024.
- **TUI library**: Ratatui 0.30 confirmed after exhaustive research (vs cursive, r3bl_tui, iocraft, tui-realm). All kanban TUIs, all reference projects use ratatui.
- **Mouse support**: Click-to-select (no drag-and-drop), mouse wheel scroll (hover-based: column under cursor), click outside modal to dismiss, one-time tmux mouse hint.

**Research Findings**:
- **Agent of Empires** (650 stars): Closest prior art. Rust + ratatui + tmux + worktree. Has proven OpenCode status detection patterns. Uses list/tree view, not kanban. Reference: `src/tmux/status_detection.rs`.
- **claude-tmux** (24 stars): Ratatui TUI for Claude Code tmux sessions. Simple architecture: `main.rs → app.rs → ui.rs → tmux.rs → session.rs → detection.rs → input.rs`.
- **twig** (28 stars): Tmux control mode, git worktree + tmux session manager with YAML config.
- **orchestra** (24 stars): Rust TUI for parallel AI agents + worktrees.
- **Ratatui 0.30.0**: Industry standard TUI framework. Immediate-mode rendering. `Widget`/`StatefulWidget` traits.
- **OpenCode sessions**: Stored in SQLite, ID format = UUID v4, resume via `-s <session_id>` flag.

### Metis Review (Round 1 — Original Plan)
**Identified Gaps** (addressed in plan):
1. **Tmux server isolation**: Use dedicated socket `-L opencode-kanban` to avoid collision with user's existing sessions. → Addressed in Task 4.
2. **Stable identifiers**: Use UUID `task_id` + stable `repo_id` in DB, separate from display names. Don't rely on `repo-branch` alone. → Addressed in Task 2.
3. **Session name sanitization**: Tmux names limited to alphanumeric + hyphen + underscore, max ~200 chars. Store unsanitized display names in SQLite. → Addressed in Task 4.
4. **Non-shell execution**: Always `Command::new("git").args([...])`, never `sh -c`. → Guardrail applied globally.
5. **Default base branch**: Detect repo's HEAD (could be `main`, `master`, `trunk`), not hardcode. → Addressed in Task 3.
6. **Edge cases**: Duplicate branch names across repos, non-ASCII chars, missing repos, worktree conflicts. → Addressed in Task 3 acceptance criteria.
7. **Polling performance**: Backoff/jitter for status detection when many tasks. → Addressed in Task 6.
8. **Outside-tmux behavior**: Fail fast with actionable error if run outside tmux. → Addressed in Task 1.

### Metis Review (Round 2 — Mouse Support + TUI Library)
**Identified Gaps** (addressed in plan update):
9. **Mouse z-order routing**: Modals must receive events first; underlying board must not get clicks. → Added to Must NOT Have guardrails + Task 5 hit-test map.
10. **Hover-based scroll**: Mouse wheel scrolls column under cursor (not focused column). → Added to Task 5 mouse support + QA tests.
11. **Modal dismiss on outside click**: Click outside modal = Esc. → Added to Task 5 mouse behavior + QA tests.
12. **Tmux mouse hint**: Mouse events need `tmux set -g mouse on`. No config modification — one-time hint only. → Added to Task 10 polish.
13. **Keyboard+mouse focus consistency**: After mouse click, keyboard must continue from mouse-selected position. → Added test to Task 5 QA.
14. **Scope creep locks**: No drag-and-drop, hover effects, context menus, double-click, mouse resize. → All added to Must NOT Have.
15. **Border clicks inert**: Clicks on Block borders must not activate items. → Added to guardrails + Task 5 hit-test logic.
16. **Scroll clamping**: Touchpad burst events must not cause underflow/overflow. → Added to Task 5 + QA tests.

**TUI Library Confirmed**: Ratatui 0.30 validated via exhaustive research (18K stars, all kanban TUIs use it, TEA alignment, polling model fit, fulsomenko/kanban as direct prior art). No change needed.

---

## Work Objectives

### Core Objective
Build a Rust TUI kanban board (`opencode-kanban`) that enables developers to manage multiple parallel AI coding sessions across different repositories, with each task tied to a git worktree + OpenCode instance in its own tmux session.

### Concrete Deliverables
- `opencode-kanban` binary (single Rust binary)
- SQLite database at platform-appropriate data dir
- Worktree directory structure under centralized base
- Full TUI with horizontal kanban columns, task CRUD, keyboard navigation
- OpenCode integration with session resume and status detection

### Definition of Done
- [ ] `cargo build --release` produces working binary
- [ ] `cargo test` passes all tests (unit + integration)
- [ ] `cargo clippy -- -D warnings` clean
- [ ] Can create a task, attach to OpenCode session, detach, see status, move between columns, delete with cleanup
- [ ] Survives simulated crash (kill tmux sessions, restart app, see dead status, re-attach recreates)

### Must Have
- Horizontal kanban board with scrollable columns
- Task CRUD (create, read/view, move between columns, delete with cleanup)
- Git worktree creation from any registered repo
- Tmux session per task with OpenCode running
- OpenCode session ID persistence and resume across reboots
- Status detection (running/waiting/idle/dead)
- Auto-refresh polling
- Keyboard help panel
- Configurable categories (add/rename/remove columns)
- Task reordering within columns
- **Mouse support**: all interactable elements must be clickable
  - Click task cards to select (focus column + highlight card)
  - Click column headers to focus column
  - Click dialog buttons (Create, Cancel, Delete, etc.)
  - Click checkbox items in delete confirmation dialog
  - Click list items in repo/branch selectors
  - Mouse wheel scrolls card list in the column **under the cursor** (hover-based)
  - Click outside modal dialog to dismiss it (same as Esc)
  - One-time hint if mouse events not received: "Enable tmux mouse: `tmux set -g mouse on`"

### Must NOT Have (Guardrails)
- No PR creation or git push operations (v2+)
- No git status/diff display on task cards (v2+)
- No task notes, comments, or rich text (v2+)
- No task filtering or search (v2+)
- No CLI interface (TUI-only for v1)
- No multi-tool support (OpenCode only, no Claude Code/Codex/etc.)
- No Docker sandboxing
- No `sh -c` or string-interpolated shell commands — always `Command::new().args([])`
- No implicit network operations without user action (fetch is explicit in task creation flow)
- No modifications to user's git config or tmux config (no `tmux set-option` — only show hints)
- No hardcoded "main" as default branch — detect repo's HEAD
- No Windows support (Linux/macOS only; fail fast with error on unsupported OS)
- **Mouse guardrails** (from Metis review):
  - No drag-and-drop (`MouseEventKind::Drag` ignored entirely) — v2+
  - No double-click semantics — single `Down(Left)` only
  - No right-click context menus — v2+
  - No hover effects, tooltips, or mouseover previews — v2+
  - No mouse-driven resize of columns/panels
  - All hit-testing must be deterministic from layout `Rect`s, not buffer content parsing
  - Mouse actions must route through same TEA update pipeline as keyboard (keyboard parity)
  - Clicks on `Block` borders are inert (only inner content is clickable)
  - z-order: modals receive mouse events first; clicks do NOT pass through to underlying board

---

## Verification Strategy

> **UNIVERSAL RULE: ZERO HUMAN INTERVENTION**
>
> ALL tasks MUST be verifiable WITHOUT any human action.

### Test Decision
- **Infrastructure exists**: NO (greenfield project)
- **Automated tests**: TDD (test-first)
- **Framework**: `cargo test` (built-in Rust test framework)
- **Setup**: Task 1 includes test infrastructure setup

### If TDD Enabled

Each TODO follows RED-GREEN-REFACTOR:

**Task Structure:**
1. **RED**: Write failing test first
   - Test file: `tests/<module>.rs` or `src/<module>/tests.rs`
   - Test command: `cargo test <test_name>`
   - Expected: FAIL (test exists, implementation doesn't)
2. **GREEN**: Implement minimum code to pass
   - Command: `cargo test <test_name>`
   - Expected: PASS
3. **REFACTOR**: Clean up while keeping green
   - Command: `cargo test`
   - Expected: PASS (still)

**Test Setup Task:**
- [ ] 0. Included in Task 1 (Project Scaffold)
  - `cargo init` with edition 2024
  - `Cargo.toml` with dev-dependencies for testing
  - Verify: `cargo test` → 0 tests, passes

### Agent-Executed QA Scenarios (MANDATORY — ALL tasks)

**Verification Tool by Deliverable Type:**

| Type | Tool | How Agent Verifies |
|------|------|-------------------|
| **TUI rendering** | interactive_bash (tmux) | Run binary in tmux, send keystrokes, capture pane, assert content |
| **Git operations** | Bash (git commands) | Create test repos, run operations, verify worktree/branch state |
| **SQLite persistence** | Bash (sqlite3 CLI) | Query DB directly, assert rows/columns |
| **Tmux session mgmt** | Bash (tmux commands) | List sessions, capture panes, verify process state |
| **OpenCode integration** | Bash (tmux + process check) | Verify opencode process running in correct pane with correct args |

---

## Execution Strategy

### Parallel Execution Waves

```
Wave 1 (Start Immediately):
└── Task 1: Project scaffold + TUI shell (no dependencies)

Wave 2 (After Wave 1):
├── Task 2: SQLite persistence layer (depends: 1)
├── Task 3: Git worktree operations module (depends: 1)
└── Task 4: Tmux session management module (depends: 1)

Wave 3 (After Wave 2):
├── Task 5: Kanban board UI + task CRUD (depends: 2)
├── Task 6: OpenCode integration + status detection (depends: 2, 4)
└── Task 7: Crash recovery + reconciliation (depends: 2, 4)

Wave 4 (After Wave 3):
├── Task 8: Task creation flow (full pipeline) (depends: 3, 4, 5)
├── Task 9: Configurable categories + task reordering (depends: 5)
└── Task 10: Help panel + polish + integration tests (depends: 5, 6, 7, 8)

Critical Path: Task 1 → Task 2 → Task 5 → Task 8 → Task 10
```

### Dependency Matrix

| Task | Depends On | Blocks | Can Parallelize With |
|------|------------|--------|---------------------|
| 1 | None | 2, 3, 4 | None (foundation) |
| 2 | 1 | 5, 6, 7 | 3, 4 |
| 3 | 1 | 8 | 2, 4 |
| 4 | 1 | 6, 7, 8 | 2, 3 |
| 5 | 2 | 8, 9, 10 | 6, 7 |
| 6 | 2, 4 | 10 | 5, 7 |
| 7 | 2, 4 | 10 | 5, 6 |
| 8 | 3, 4, 5 | 10 | 9 |
| 9 | 5 | 10 | 8 |
| 10 | 5, 6, 7, 8 | None (final) | None |

### Agent Dispatch Summary

| Wave | Tasks | Recommended Agents |
|------|-------|-------------------|
| 1 | 1 | `task(category="unspecified-high", load_skills=[], ...)` |
| 2 | 2, 3, 4 | Dispatch all 3 in parallel after Wave 1 |
| 3 | 5, 6, 7 | Dispatch all 3 in parallel after Wave 2 |
| 4 | 8, 9, 10 | 8 and 9 parallel, then 10 last |

---

## TODOs

- [ ] 1. Project Scaffold + TUI Shell

  **What to do**:
  - Run `cargo init --name opencode-kanban` in `/home/cc/codes/opencode-kanban` with edition 2024
  - Set up `Cargo.toml` with initial dependencies:
    - `ratatui = "0.30"` (TUI framework)
    - `crossterm = "0.28"` (terminal backend)
    - `rusqlite = { version = "0.32", features = ["bundled"] }` (SQLite)
    - `serde = { version = "1", features = ["derive"] }` (serialization)
    - `serde_json = "1"` (JSON for config)
    - `anyhow = "1"` (error handling)
    - `tokio = { version = "1", features = ["full"] }` (async runtime for polling)
    - `dirs = "6"` (cross-platform paths)
    - `uuid = { version = "1", features = ["v4"] }` (task/repo IDs)
    - `chrono = { version = "0.4", features = ["serde"] }` (timestamps)
    - `tracing = "0.1"` + `tracing-subscriber = "0.3"` (logging)
  - Create `.gitignore` with standard Rust template + `/target`, `*.sqlite`, `*.sqlite-wal`, `*.sqlite-shm`
  - Create basic project structure:
    ```
    src/
    ├── main.rs          # Entry point, terminal setup, tmux-inside check
    ├── app.rs           # TEA: App state, Message enum, update()
    ├── ui.rs            # TEA: view/render functions
    ├── input.rs         # Keyboard event handling
    ├── db/
    │   └── mod.rs       # SQLite persistence (Task 2)
    ├── git/
    │   └── mod.rs       # Git worktree ops (Task 3)
    ├── tmux/
    │   └── mod.rs       # Tmux session mgmt (Task 4)
    ├── opencode/
    │   └── mod.rs       # OpenCode integration (Task 6)
    └── types.rs         # Shared types: Task, Category, Repo, Status
    ```
  - Implement `main.rs`:
    - Check if running inside tmux (`$TMUX` env var). If not, print error with instructions and exit.
    - Check platform is Linux/macOS. Fail fast on unsupported OS.
    - Initialize terminal (crossterm alternate screen, raw mode, **`EnableMouseCapture`**)
    - On exit: restore terminal with **`DisableMouseCapture`** (also in panic hook)
    - Create basic event loop (TEA pattern): poll crossterm events → update state → render
    - Handle `Event::Key` for keyboard, `Event::Mouse` for mouse (both route to same `update()`)
    - Handle `Event::Resize` to invalidate/regenerate layout
    - Render a placeholder kanban board with 3 empty columns (TODO | IN PROGRESS | DONE)
    - Handle `q` to quit cleanly (restore terminal)
  - Write initial test: `cargo test` runs and passes (even if just a trivial assertion)
  - Create `rust-toolchain.toml` with `channel = "stable"`

  **Must NOT do**:
  - Don't implement any actual DB, git, tmux, or OpenCode functionality yet
  - Don't add CLI arguments (TUI-only)
  - Don't implement mouse click handlers (just enable capture + forward events; Task 5 adds handlers)
  - Don't use `sh -c` for any command execution

  **Recommended Agent Profile**:
  - **Category**: `unspecified-high`
    - Reason: Greenfield Rust project setup requires careful Cargo.toml configuration, TEA architecture scaffolding, and correct terminal setup. Not purely UI, not purely logic.
  - **Skills**: []
    - No special skills needed — standard Rust development
  - **Skills Evaluated but Omitted**:
    - `frontend-ui-ux`: Not needed yet — this task is scaffold only, no visual design

  **Parallelization**:
  - **Can Run In Parallel**: NO
  - **Parallel Group**: Wave 1 (solo)
  - **Blocks**: Tasks 2, 3, 4
  - **Blocked By**: None (can start immediately)

  **References**:

  **Pattern References**:
  - `nielsgroen/claude-tmux` project structure: `main.rs → app.rs → ui.rs → tmux.rs → session.rs → detection.rs → input.rs` — Follow this flat module layout for v1, it's proven for similar-scope TUI apps.
  - Ratatui component template: `https://github.com/ratatui/templates/tree/main/component` — Reference for TEA pattern in ratatui.

  **API/Type References**:
  - `ratatui::prelude::*` — Core ratatui imports: `Frame`, `Rect`, `Layout`, `Constraint`, `Widget`, `StatefulWidget`, `Buffer`
  - `crossterm::event::*` — Event polling: `Event::Key(KeyEvent { code, modifiers, .. })`
  - `dirs::data_dir()` — Returns platform-appropriate data directory

  **External References**:
  - Ratatui 0.30 docs: `https://docs.rs/ratatui/0.30.0/ratatui/`
  - Crossterm docs: `https://docs.rs/crossterm/latest/crossterm/`

  **WHY Each Reference Matters**:
  - claude-tmux structure: Proves this flat layout scales for our scope. Avoid over-engineering with nested modules.
  - Ratatui template: Shows canonical TEA pattern with `App::new()`, `App::run()`, event loop, and clean terminal restore on panic.

  **Acceptance Criteria**:

  **TDD:**
  - [ ] `cargo test` → PASS (at least 1 test: platform check or trivial app state test)
  - [ ] `cargo clippy -- -D warnings` → clean

  **Agent-Executed QA Scenarios:**

  ```
  Scenario: Binary builds and runs inside tmux
    Tool: Bash + interactive_bash (tmux)
    Preconditions: Rust toolchain installed, project directory exists
    Steps:
      1. cargo build --release 2>&1
      2. Assert: exit code 0, binary exists at target/release/opencode-kanban
      3. tmux new-session -d -s test-scaffold
      4. tmux send-keys -t test-scaffold "TMUX_ORIG=$TMUX && cd /home/cc/codes/opencode-kanban && ./target/release/opencode-kanban" Enter
      5. sleep 2
      6. tmux capture-pane -t test-scaffold -p
      7. Assert: output contains "TODO" AND "IN PROGRESS" AND "DONE"
      8. tmux send-keys -t test-scaffold "q"
      9. tmux kill-session -t test-scaffold
    Expected Result: Binary runs, shows 3 kanban columns, exits cleanly on 'q'
    Evidence: Terminal output captured

  Scenario: Binary fails gracefully outside tmux
    Tool: Bash
    Preconditions: Binary built
    Steps:
      1. unset TMUX && ./target/release/opencode-kanban 2>&1
      2. Assert: exit code != 0
      3. Assert: stderr contains "tmux" (instructional error message)
    Expected Result: Clear error message about needing tmux
    Evidence: stderr output captured

  Scenario: Mouse capture enabled on startup
    Tool: interactive_bash (tmux)
    Preconditions: Binary built, tmux has `set -g mouse on`
    Steps:
      1. tmux -L opencode-kanban-test new-session -d -s test-mouse -x 80 -y 24
      2. tmux -L opencode-kanban-test send-keys -t test-mouse "cd /home/cc/codes/opencode-kanban && ./target/release/opencode-kanban" Enter
      3. sleep 2
      4. tmux -L opencode-kanban-test capture-pane -t test-mouse -p
      5. Assert: board renders (contains "TODO")
      6. tmux -L opencode-kanban-test send-keys -t test-mouse "q"
      7. tmux -L opencode-kanban-test kill-session -t test-mouse
    Expected Result: App starts with mouse capture enabled, exits cleanly restoring mouse state
    Evidence: Terminal output captured
  ```

  **Commit**: YES
  - Message: `feat: initial project scaffold with ratatui TUI shell and TEA architecture`
  - Files: `Cargo.toml`, `Cargo.lock`, `src/**`, `.gitignore`, `rust-toolchain.toml`
  - Pre-commit: `cargo test && cargo clippy -- -D warnings`

---

- [ ] 2. SQLite Persistence Layer

  **What to do**:
  - Design and implement the SQLite schema in `src/db/mod.rs`:
    ```sql
    CREATE TABLE repos (
      id TEXT PRIMARY KEY,              -- UUID v4
      path TEXT NOT NULL UNIQUE,        -- Absolute filesystem path to repo root
      name TEXT NOT NULL,               -- Display name (derived from dir basename)
      default_base TEXT,                -- Default base branch (detected from HEAD, e.g., "main")
      remote_url TEXT,                  -- origin remote URL (for display/identification)
      created_at TEXT NOT NULL,         -- ISO 8601 timestamp
      updated_at TEXT NOT NULL
    );

    CREATE TABLE categories (
      id TEXT PRIMARY KEY,              -- UUID v4
      name TEXT NOT NULL UNIQUE,        -- Display name (e.g., "TODO", "IN PROGRESS", "DONE")
      position INTEGER NOT NULL,        -- Sort order (0-indexed)
      created_at TEXT NOT NULL
    );

    CREATE TABLE tasks (
      id TEXT PRIMARY KEY,              -- UUID v4
      title TEXT NOT NULL,              -- User-provided title (defaults to repo:branch)
      repo_id TEXT NOT NULL REFERENCES repos(id),
      branch TEXT NOT NULL,             -- Git branch name (unsanitized, for git ops)
      category_id TEXT NOT NULL REFERENCES categories(id),
      position INTEGER NOT NULL,        -- Sort order within category (0-indexed)
      tmux_session_name TEXT,           -- Sanitized tmux session name (ok-{repo}-{branch})
      opencode_session_id TEXT,         -- OpenCode UUID v4 session ID (for resume)
      worktree_path TEXT,               -- Absolute path to the worktree directory
      tmux_status TEXT DEFAULT 'unknown', -- running/waiting/idle/dead/unknown
      created_at TEXT NOT NULL,
      updated_at TEXT NOT NULL,
      UNIQUE(repo_id, branch)           -- One task per repo+branch combo
    );
    ```
  - Seed default categories on first run: TODO (pos 0), IN PROGRESS (pos 1), DONE (pos 2)
  - Implement CRUD operations as a `Database` struct:
    - `Database::open(path)` — open/create DB, run migrations
    - `Database::add_repo(path) -> Repo` — register a repo (detect name from basename, detect default_base from `git symbolic-ref refs/remotes/origin/HEAD`)
    - `Database::list_repos() -> Vec<Repo>`
    - `Database::add_task(repo_id, branch, title, category_id) -> Task`
    - `Database::get_task(id) -> Task`
    - `Database::list_tasks() -> Vec<Task>`
    - `Database::update_task_category(id, category_id, position)`
    - `Database::update_task_position(id, position)`
    - `Database::update_task_tmux(id, tmux_session_name, opencode_session_id, worktree_path)`
    - `Database::update_task_status(id, status)`
    - `Database::delete_task(id)`
    - Category CRUD: `add_category`, `list_categories`, `update_category_position`, `rename_category`, `delete_category`
  - All operations use parameterized queries (no string interpolation)
  - Use `anyhow::Result` for error handling
  - Write comprehensive TDD tests using in-memory SQLite (`:memory:`)

  **Must NOT do**:
  - Don't add any migration framework (manual CREATE TABLE is fine for v1)
  - Don't add any caching layer
  - Don't make DB operations async (synchronous rusqlite is fine)
  - Don't add any CLI for DB operations

  **Recommended Agent Profile**:
  - **Category**: `unspecified-high`
    - Reason: SQLite schema design + Rust data layer with comprehensive TDD tests. Requires careful type design.
  - **Skills**: []
  - **Skills Evaluated but Omitted**:
    - `ultrabrain`: Schema is straightforward, doesn't need heavy logic

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 2 (with Tasks 3, 4)
  - **Blocks**: Tasks 5, 6, 7
  - **Blocked By**: Task 1

  **References**:

  **Pattern References**:
  - `njbrake/agent-of-empires` `src/session/instance.rs` — Status enum (`Running`, `Waiting`, `Idle`, `Error`) and session data model.
  - `rusqlite` parameterized queries pattern: `conn.execute("INSERT INTO x (a,b) VALUES (?1, ?2)", params![a, b])`

  **API/Type References**:
  - `rusqlite::Connection::open(path)`, `Connection::open_in_memory()` (for tests)
  - `rusqlite::params![]` macro for safe parameter binding
  - `uuid::Uuid::new_v4().to_string()` for ID generation

  **External References**:
  - rusqlite docs: `https://docs.rs/rusqlite/latest/rusqlite/`
  - SQLite data types: `https://www.sqlite.org/datatype3.html`

  **WHY Each Reference Matters**:
  - AoE session instance: Establishes the proven Status enum we should match
  - rusqlite params: Ensures SQL injection safety without ORMs

  **Acceptance Criteria**:

  **TDD:**
  - [ ] Tests for: DB creation, repo CRUD, task CRUD, category CRUD, unique constraints, cascading behavior
  - [ ] `cargo test db` → PASS (all DB tests green)

  **Agent-Executed QA Scenarios:**

  ```
  Scenario: Database file created at correct platform path
    Tool: Bash
    Preconditions: Binary built with DB module integrated
    Steps:
      1. Run opencode-kanban briefly (in tmux, then quit)
      2. Check data dir: ls $(dirs::data_dir analog)/opencode-kanban/
      3. Assert: opencode-kanban.sqlite exists
      4. sqlite3 <db_path> ".tables"
      5. Assert: output contains "repos" AND "tasks" AND "categories"
      6. sqlite3 <db_path> "SELECT name, position FROM categories ORDER BY position"
      7. Assert: output shows TODO|0, IN PROGRESS|1, DONE|2
    Expected Result: DB created with schema and default categories
    Evidence: sqlite3 output captured

  Scenario: Unique constraint prevents duplicate repo+branch
    Tool: Bash (cargo test)
    Preconditions: DB module implemented
    Steps:
      1. cargo test test_duplicate_repo_branch
      2. Assert: test passes (adding same repo+branch twice returns error)
    Expected Result: Constraint enforced at DB level
    Evidence: Test output captured
  ```

  **Commit**: YES
  - Message: `feat(db): SQLite persistence layer with schema, migrations, and CRUD operations`
  - Files: `src/db/mod.rs`, `src/types.rs`
  - Pre-commit: `cargo test && cargo clippy -- -D warnings`

---

- [ ] 3. Git Worktree Operations Module

  **What to do**:
  - Implement `src/git/mod.rs` with all git operations via `Command::new("git").args([...])`:
    - `git_detect_default_branch(repo_path) -> String` — Run `git symbolic-ref refs/remotes/origin/HEAD` → parse branch name. Fallback: try `main`, then `master`, then first branch.
    - `git_fetch(repo_path) -> Result<()>` — `git fetch origin` in the repo dir
    - `git_list_branches(repo_path) -> Vec<Branch>` — `git branch -a --format=...` for both local and remote branches
    - `git_list_tags(repo_path) -> Vec<String>` — `git tag -l`
    - `git_create_worktree(repo_path, worktree_path, branch_name, base_ref) -> Result<()>`:
      1. Validate `branch_name` with `git check-ref-format --branch`
      2. Check if worktree already exists at `worktree_path`
      3. Run `git worktree add -b <branch_name> <worktree_path> <base_ref>`
      4. Return error with context on failure
    - `git_remove_worktree(repo_path, worktree_path) -> Result<()>` — `git worktree remove <path>` (with `--force` option)
    - `git_delete_branch(repo_path, branch_name) -> Result<()>` — `git branch -d <branch>` (safe delete, not force)
    - `git_is_valid_repo(path) -> bool` — `git -C <path> rev-parse --git-dir`
    - `git_get_remote_url(repo_path) -> Option<String>` — `git remote get-url origin`
  - Worktree path derivation: `{base_dir}/{repo_slug}/{branch_slug}` where:
    - `repo_slug` = repo directory basename, sanitized (replace non-alphanum with `-`)
    - `branch_slug` = branch name, sanitized (replace `/` and non-alphanum with `-`)
    - Handle collisions: if path exists, append `-2`, `-3`, etc.
  - ALL commands use `std::process::Command` with `.args([])`, NEVER `sh -c`
  - ALL commands set `.current_dir(repo_path)` where appropriate
  - Capture and parse both stdout and stderr for meaningful error messages
  - Write TDD tests using temporary git repositories (create test repos with `git init` in temp dirs)

  **Must NOT do**:
  - Don't use `git2` crate — shell out to git CLI only
  - Don't push or pull (no network operations beyond fetch)
  - Don't modify user's git config
  - Don't force-delete branches (only safe delete)
  - Don't handle submodules or bare repos (out of scope)

  **Recommended Agent Profile**:
  - **Category**: `unspecified-high`
    - Reason: Git worktree operations with careful edge case handling, process execution, and comprehensive testing with temp repos.
  - **Skills**: []
  - **Skills Evaluated but Omitted**:
    - `git-master`: This is about programmatic git operations from Rust, not interactive git workflow

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 2 (with Tasks 2, 4)
  - **Blocks**: Task 8
  - **Blocked By**: Task 1

  **References**:

  **Pattern References**:
  - `andersonkrs/twig` `src/git.rs:create_worktree()` — Worktree creation with branch_safe slug (`replace('/', "-")`), parent dir creation, and error context.
  - `njbrake/agent-of-empires` `src/git/worktree.rs:create_worktree()` — Branch validation, base ref resolution, and `Command::new("git")` pattern.
  - `2mawi2/schaltwerk` `src-tauri/src/domains/git/worktrees.rs:create_worktree_from_base()` — Fallback logic when base branch is missing.

  **API/Type References**:
  - `std::process::Command` — `.new("git").args(["worktree", "add", ...]).current_dir(repo_path).output()?`
  - `std::process::Output` — `.status.success()`, `String::from_utf8_lossy(&output.stderr)`

  **External References**:
  - `git worktree` docs: `https://git-scm.com/docs/git-worktree`
  - `git check-ref-format` docs: `https://git-scm.com/docs/git-check-ref-format`

  **WHY Each Reference Matters**:
  - twig's create_worktree: Shows the canonical Rust pattern for worktree creation with slug generation
  - AoE's worktree module: Shows branch validation and fallback logic for base ref resolution
  - schaltwerk's pattern: Shows bootstrapping from HEAD when base branch is missing

  **Acceptance Criteria**:

  **TDD:**
  - [ ] Tests for: repo validation, default branch detection, worktree create/remove, branch create/delete, slug generation, edge cases (spaces in paths, non-ASCII branch names, collisions)
  - [ ] `cargo test git` → PASS

  **Agent-Executed QA Scenarios:**

  ```
  Scenario: Create worktree from origin/main
    Tool: Bash
    Preconditions: Test git repo with commits and origin/main
    Steps:
      1. Create temp repo: git init /tmp/test-repo && cd /tmp/test-repo && git commit --allow-empty -m "init" && git checkout -b main
      2. cargo test test_create_worktree_from_base
      3. Assert: test passes
      4. Assert: worktree directory exists at expected path
      5. Assert: branch exists in git branch output
      6. Assert: git worktree list includes the new worktree
    Expected Result: Worktree created with correct branch tracking
    Evidence: Test output + git worktree list output

  Scenario: Duplicate branch across repos doesn't collide
    Tool: Bash (cargo test)
    Preconditions: Two test repos
    Steps:
      1. cargo test test_worktree_path_no_collision
      2. Assert: worktree paths are different (include repo slug)
    Expected Result: Paths are unique per repo
    Evidence: Test output

  Scenario: Invalid branch name is rejected
    Tool: Bash (cargo test)
    Preconditions: Git module implemented
    Steps:
      1. cargo test test_invalid_branch_name
      2. Assert: test passes (branch names with spaces, "..", "~" are rejected)
    Expected Result: Validation prevents bad branch names
    Evidence: Test output
  ```

  **Commit**: YES
  - Message: `feat(git): worktree and branch management via git CLI with validation`
  - Files: `src/git/mod.rs`
  - Pre-commit: `cargo test && cargo clippy -- -D warnings`

---

- [ ] 4. Tmux Session Management Module

  **What to do**:
  - Implement `src/tmux/mod.rs` with all tmux operations via `Command::new("tmux")`:
    - Use dedicated tmux socket: all commands include `-L opencode-kanban` to isolate from user's sessions
    - `tmux_session_exists(session_name) -> bool` — `tmux -L opencode-kanban has-session -t <name>`
    - `tmux_create_session(session_name, working_dir, command) -> Result<()>`:
      1. `tmux -L opencode-kanban new-session -d -s <name> -c <working_dir>`
      2. If command provided: `tmux -L opencode-kanban send-keys -t <name> "<command>" Enter`
    - `tmux_kill_session(session_name) -> Result<()>` — `tmux -L opencode-kanban kill-session -t <name>`
    - `tmux_switch_client(session_name) -> Result<()>` — `tmux -L opencode-kanban switch-client -t <name>`
    - `tmux_list_sessions() -> Vec<TmuxSession>` — `tmux -L opencode-kanban list-sessions -F "#{session_name}\t#{session_created}\t#{session_attached}"`
    - `tmux_capture_pane(session_name, lines) -> Result<String>` — `tmux -L opencode-kanban capture-pane -t <name>:0.0 -p -S -<lines>`
    - `tmux_get_pane_pid(session_name) -> Option<u32>` — `tmux -L opencode-kanban list-panes -t <name> -F "#{pane_pid}"`
  - Session name sanitization function:
    - Input: `(repo_name, branch_name)` → Output: `ok-{sanitized_repo}-{sanitized_branch}`
    - Replace non-alphanumeric chars (except `-`) with `-`
    - Truncate to 200 chars max
    - Store unsanitized names separately (in SQLite `tasks.branch` field)
  - Check tmux is installed on startup, fail fast with actionable error
  - ALL commands use `Command::new("tmux").args(["-L", "opencode-kanban", ...])`, NEVER `sh -c`
  - Write TDD tests (may need to mock tmux or use actual tmux in test env)

  **Must NOT do**:
  - Don't use `tmux_interface` crate — shell out directly (simpler, proven by claude-tmux)
  - Don't use tmux control mode (overkill for our needs)
  - Don't modify user's tmux config
  - Don't use default tmux server (always use `-L opencode-kanban` socket)

  **Recommended Agent Profile**:
  - **Category**: `unspecified-high`
    - Reason: Tmux session management with careful isolation, process execution, and edge case handling for session naming.
  - **Skills**: []
  - **Skills Evaluated but Omitted**:
    - `playwright`: Not browser-based

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 2 (with Tasks 2, 3)
  - **Blocks**: Tasks 6, 7, 8
  - **Blocked By**: Task 1

  **References**:

  **Pattern References**:
  - `nielsgroen/claude-tmux` `src/tmux.rs` — `Tmux::list_sessions()` pattern with `Command::new("tmux").args(["list-sessions", "-F", format_string])`. Simple, proven approach.
  - `njbrake/agent-of-empires` `src/tmux/session.rs:detect_status()` — `capture_pane(50)` + `get_foreground_pid()` pattern.

  **API/Type References**:
  - `std::process::Command` with `.args(["-L", "opencode-kanban", "new-session", "-d", "-s", name])`
  - Tmux format strings: `#{session_name}`, `#{session_created}`, `#{pane_pid}`

  **External References**:
  - tmux man page: `https://man7.org/linux/man-pages/man1/tmux.1.html`
  - tmux session naming rules: alphanumeric, `-`, `_` only; no `.` (used as separator)

  **WHY Each Reference Matters**:
  - claude-tmux tmux.rs: Shows the simplest correct pattern for tmux operations from Rust
  - AoE session.rs: Shows pane capture + PID detection for status polling

  **Acceptance Criteria**:

  **TDD:**
  - [ ] Tests for: session name sanitization (edge cases: spaces, unicode, long names, `/` in branch), create/kill/list sessions, capture pane
  - [ ] `cargo test tmux` → PASS

  **Agent-Executed QA Scenarios:**

  ```
  Scenario: Create and verify isolated tmux session
    Tool: Bash
    Preconditions: tmux installed
    Steps:
      1. cargo test test_tmux_create_session (integration test that creates a real tmux session)
      2. tmux -L opencode-kanban list-sessions
      3. Assert: test session appears in list
      4. tmux -L opencode-kanban list-sessions (default server)
      5. Assert: test session does NOT appear in default server
      6. Cleanup: tmux -L opencode-kanban kill-server
    Expected Result: Sessions isolated to dedicated socket
    Evidence: tmux list-sessions output from both servers

  Scenario: Session name sanitization handles edge cases
    Tool: Bash (cargo test)
    Steps:
      1. cargo test test_sanitize_session_name
      2. Assert: "ok-my-repo-feature/login" → "ok-my-repo-feature-login"
      3. Assert: "ok-my repo-feat ure" → "ok-my-repo-feat-ure"
      4. Assert: very long names are truncated to ≤200 chars
    Expected Result: All edge cases sanitized correctly
    Evidence: Test output
  ```

  **Commit**: YES
  - Message: `feat(tmux): session management with dedicated socket isolation`
  - Files: `src/tmux/mod.rs`
  - Pre-commit: `cargo test && cargo clippy -- -D warnings`

---

- [ ] 5. Kanban Board UI + Task CRUD

  **What to do**:
  - Implement the full kanban board rendering in `src/ui.rs` per this exact layout spec:

    **Target Layout (80x24 minimum):**
    ```
    ┌─ opencode-kanban ──────────────────────── 3 tasks ─ auto-refresh: 3s ─┐  ← Header (1 line)
    │                                                                        │
    │  ┌─ TODO (1) ────────┐ ┌─ IN PROGRESS (1) ═══╗ ┌─ DONE (1) ────────┐ │  ← Column headers
    │  │                    │ ║                      ║ │                    │ │
    │  │  ● Add login page  │ ║ ▸○ Refactor auth    ║ │  ○ Fix typo       │ │  ← Task cards
    │  │    myapp:feat/login│ ║    backend:ref-auth  ║ │    docs:fix-typo  │ │     (2 lines each)
    │  │                    │ ║                      ║ │                    │ │
    │  │                    │ ║  ◐ Add caching       ║ │                    │ │
    │  │                    │ ║    backend:add-cache  ║ │                    │ │
    │  │                    │ ║                      ║ │                    │ │
    │  └────────────────────┘ ╚══════════════════════╝ └────────────────────┘ │
    │                                                                        │
    │  n: new  d: delete  m: move  Enter: attach  J/K: reorder  ?: help     │  ← Status bar (1 line)
    └────────────────────────────────────────────────────────────────────────┘
    ```

    **Layout Rules:**
    - **Overall**: 3 rows — header (Height 1), columns area (Fill), status bar (Height 1)
    - **Columns**: `Layout::horizontal()` with `Constraint::Ratio(1, N)` where N = number of categories
    - **Column border**: Normal `Block::bordered()` for unfocused, double-line/colored border for focused column
    - **Card selection**: `▸` prefix + highlighted background for selected card in focused column
    - **Card format** (2 lines per card):
      - Line 1: `[status_icon] [title]` — icon colored, title in normal style
      - Line 2: `  [repo]:[branch]` — dimmed/gray, indented 2 spaces
    - **Status icons** (colored Unicode):
      - `●` Green = Running (opencode is processing)
      - `○` White = Idle (opencode waiting for input)
      - `◐` Yellow = Waiting (permission prompt / y/n)
      - `✕` Red = Dead (tmux session gone)
      - `?` DarkGray = Unknown (default / detection failed)
    - **Truncation**: If terminal < 80 wide, show warning. Names truncated with `…` to fit column width.
    - **Column header**: `─ CATEGORY_NAME (count) ─` centered in border title

    **New Task Dialog (modal overlay, centered):**
    ```
    ┌─ New Task ────────────────────────────────┐
    │                                            │
    │  Repo:   [~/codes/myapp         ▾]        │  ← Known repos list + "add new" option
    │  Branch: [feature/_______________]        │  ← Text input with validation
    │  Base:   [origin/main           ▾]        │  ← Pre-filled from repo's HEAD, editable
    │  Title:  [________________________]       │  ← Optional (defaults to repo:branch)
    │                                            │
    │         [Create]    [Cancel]               │
    │                                            │
    │  Tab: next field  Enter: confirm           │
    └────────────────────────────────────────────┘
    ```

    **Delete Confirmation Dialog:**
    ```
    ┌─ Delete Task ─────────────────────────────┐
    │                                            │
    │  Delete "Refactor auth"?                   │
    │  (backend:refactor-auth)                   │
    │                                            │
    │  Cleanup options:                          │
    │  [x] Kill tmux session                     │
    │  [ ] Remove worktree                       │
    │  [ ] Delete branch                         │
    │                                            │
    │         [Delete]    [Cancel]               │
    └────────────────────────────────────────────┘
    ```

  - Implementation details:
    - Horizontal column layout using `Layout::horizontal()` with equal `Constraint::Ratio(1, N)` per category
    - Each column: `Block::bordered()` with category name as title
    - Task cards inside columns: `List` widget with `ListState` for selection
    - Focused column has double border (`.border_type(BorderType::Double)`) or colored border
    - Selected card has highlighted background via `Style::new().bg(Color::DarkGray)`
    - **Hit-test map**: Store each rendered widget's `Rect` in `AppState` per frame (column rects, card rects, button rects). Mouse events do coordinate lookup against this map.
  - Implement mouse support in `src/input.rs`:
    - `MouseEventKind::Down(MouseButton::Left)` + hit-test:
      - Click inside task card `Rect` → `SelectTask(column_idx, task_idx)` (focuses column AND selects card)
      - Click inside column header `Rect` → `FocusColumn(column_idx)`
      - Click inside dialog button `Rect` → activate button action
      - Click inside checkbox `Rect` → toggle checkbox
      - Click inside list item `Rect` → select list item
      - Click outside modal `Rect` (when modal open) → `DismissDialog` (same as Esc)
      - Click on `Block` borders or empty space → no-op (inert)
    - `MouseEventKind::ScrollUp` / `ScrollDown`:
      - Determine which column the cursor `(column, row)` falls in
      - Scroll that column's card list up/down (hover-based, not focus-based)
      - Clamp scroll offset to valid range (handle touchpad burst events)
    - Ignore: `MouseEventKind::Drag`, `MouseEventKind::Moved`, `Down(Right)`, `Down(Middle)`
    - **All mouse actions route through the same `Message` enum as keyboard** (keyboard parity)
    - If no mouse events received for 10s after app start, show one-time hint: "Tip: `tmux set -g mouse on` enables mouse support"
  - Implement keyboard navigation in `src/input.rs`:
    - `h`/`←`: Move focus to left column
    - `l`/`→`: Move focus to right column
    - `j`/`↓`: Move selection down within column
    - `k`/`↑`: Move selection up within column
    - `Enter`: Attach to selected task's tmux session (via tmux switch-client)
    - `n`: Open "New Task" dialog
    - `d`: Open "Delete Task" confirmation dialog
    - `m` + column key: Move selected task to target column
    - `J`/`K` (shift): Reorder task within column (move up/down in position)
    - `q`: Quit
    - `?`: Toggle help panel
  - Implement modal dialogs for:
    - **New Task dialog**: Repo selector (list known repos + "Add new repo" input) → Branch name input → Base branch input (pre-filled with repo's default) → Optional title input
    - **Delete Task dialog**: Confirmation with cleanup checkboxes (kill tmux session? remove worktree? delete branch?)
    - **Move Task dialog**: Column selector (list all categories)
  - Wire UI to SQLite: load tasks/categories on startup, persist all changes immediately
  - Update `src/app.rs` TEA state machine:
    - `AppState` holds: `tasks`, `categories`, `repos`, `focused_column`, `selected_task_per_column`, `active_dialog`, `db: Database`
    - `Message` enum: `NavigateLeft`, `NavigateRight`, `SelectUp`, `SelectDown`, `AttachTask`, `CreateTask(...)`, `DeleteTask(...)`, `MoveTask(...)`, `ReorderTask(...)`, etc.

  **Must NOT do**:
  - Don't implement actual git/tmux operations in task creation (just DB + UI). Wire to real ops in Task 8.
  - Don't implement status polling yet (Task 6)
  - Don't implement configurable categories yet (Task 9)
  - Don't implement drag-and-drop, hover effects, context menus, or double-click
  - Don't add scrollbar widgets (simple list scrolling is enough)
  - Don't scatter ad-hoc mouse handlers — use single event-routing + hit-test map approach

  **Recommended Agent Profile**:
  - **Category**: `visual-engineering`
    - Reason: Core TUI rendering with layout, styling, widgets, modal dialogs. This is the visual heart of the app.
  - **Skills**: [`frontend-ui-ux`]
    - `frontend-ui-ux`: Kanban board layout, card design, navigation UX, modal dialog design — all require UI/UX sensibility even in TUI
  - **Skills Evaluated but Omitted**:
    - `playwright`: TUI, not browser

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 3 (with Tasks 6, 7)
  - **Blocks**: Tasks 8, 9, 10
  - **Blocked By**: Task 2

  **References**:

  **Pattern References**:
  - `nielsgroen/claude-tmux` `src/app/mod.rs` — `App` struct with `sessions: Vec<Session>`, `selected: usize`, `mode` enum for dialog states
  - `nielsgroen/claude-tmux` `src/ui.rs` — Ratatui rendering with `Block::bordered()`, `List::new()`, `ListState`
  - `nielsgroen/claude-tmux` `src/scroll_state.rs` — `ScrollState` wrapping `ListState` for center-locked scrolling

  **API/Type References**:
  - `ratatui::layout::Layout::horizontal()` with `Constraint::Ratio(1, num_columns)`
  - `ratatui::widgets::{Block, List, ListItem, ListState, Paragraph, Clear}`
  - `ratatui::style::{Style, Color, Modifier}`
  - `ratatui::text::{Line, Span, Text}`

  **External References**:
  - Ratatui widget gallery: `https://ratatui.rs/showcase/widgets/`
  - Ratatui layout guide: `https://ratatui.rs/concepts/layout/`

  **WHY Each Reference Matters**:
  - claude-tmux app.rs: Shows proven App state machine pattern with mode-based dialog handling
  - Ratatui layout: Essential for correct horizontal column sizing

  **Acceptance Criteria**:

  **TDD:**
  - [ ] Tests for: navigation state transitions, task move logic, reorder logic, dialog state machine
  - [ ] Tests for mouse: hit-test routing (click coord → correct action), scroll offset clamping, modal z-order blocking, keyboard+mouse focus consistency
  - [ ] `cargo test ui` → PASS (using ratatui `TestBackend` for render assertions)

  **Agent-Executed QA Scenarios:**

  ```
  Scenario: Kanban board displays tasks in correct columns with proper layout
    Tool: interactive_bash (tmux)
    Preconditions: DB seeded with test tasks in different categories
    Steps:
      1. Start opencode-kanban in tmux session (80x24 minimum)
      2. tmux capture-pane -t <session> -p
      3. Assert: first line contains "opencode-kanban" (header bar)
      4. Assert: output contains "TODO" column header with task count
      5. Assert: output contains "IN PROGRESS" column header with task count
      6. Assert: output contains "DONE" column header with task count
      7. Assert: test task titles visible in their correct columns
      8. Assert: repo:branch text visible below each title (dimmed)
      9. Assert: last line contains key hints ("n: new", "d: delete", "?: help")
      10. Assert: status icons present (●, ○, ◐, ✕, or ?)
    Expected Result: Board renders with header, 3 columns, cards with 2-line format, and status bar
    Evidence: Captured pane content

  Scenario: Navigate between columns and tasks
    Tool: interactive_bash (tmux)
    Preconditions: Board running with tasks
    Steps:
      1. tmux send-keys -t <session> "l" (move right)
      2. tmux capture-pane → Assert: IN PROGRESS column highlighted
      3. tmux send-keys -t <session> "j" (move down)
      4. tmux capture-pane → Assert: second task in column highlighted
      5. tmux send-keys -t <session> "h" (move left)
      6. tmux capture-pane → Assert: TODO column highlighted
    Expected Result: Navigation works correctly
    Evidence: Sequential pane captures

  Scenario: Create new task dialog flow
    Tool: interactive_bash (tmux)
    Steps:
      1. tmux send-keys -t <session> "n" (open new task dialog)
      2. tmux capture-pane → Assert: dialog visible with repo list
      3. (Type repo path) → (Type branch name) → (Enter to confirm)
      4. tmux capture-pane → Assert: new task appears in TODO column
    Expected Result: Task creation flow completes
    Evidence: Pane captures at each step

  Scenario: Mouse click selects task card and focuses column
    Tool: Bash (cargo test)
    Preconditions: UI module with hit-test map implemented
    Steps:
      1. cargo test test_mouse_click_selects_task
      2. Assert: clicking at coordinates inside a task card Rect produces SelectTask action
      3. Assert: focused_column updated to the clicked column
      4. Assert: selected_task_per_column updated to the clicked card index
    Expected Result: Mouse click and keyboard selection produce identical state
    Evidence: Test output

  Scenario: Mouse click on border is inert
    Tool: Bash (cargo test)
    Steps:
      1. cargo test test_mouse_click_border_inert
      2. Assert: clicking at coordinates on Block border produces no action
      3. Assert: focus state unchanged
    Expected Result: Borders don't trigger actions
    Evidence: Test output

  Scenario: Mouse wheel scrolls column under cursor
    Tool: Bash (cargo test)
    Steps:
      1. cargo test test_mouse_wheel_scrolls_hovered_column
      2. Assert: ScrollDown at coords inside column 2 scrolls column 2 (not focused column 0)
      3. Assert: scroll offset clamps at 0 (top) and max (bottom), no underflow
    Expected Result: Hover-based scroll works, clamped correctly
    Evidence: Test output

  Scenario: Click outside modal dismisses it
    Tool: Bash (cargo test)
    Steps:
      1. cargo test test_mouse_click_outside_modal_dismisses
      2. Assert: with dialog open, clicking outside modal Rect produces DismissDialog
      3. Assert: clicking inside modal does NOT dismiss
      4. Assert: clicking where a task card WOULD be (but under modal) does NOT select the card
    Expected Result: Modal z-order enforced, outside click = Esc
    Evidence: Test output

  Scenario: Mouse and keyboard focus stay consistent
    Tool: Bash (cargo test)
    Steps:
      1. cargo test test_mouse_keyboard_focus_consistency
      2. Assert: after mouse click selects column 2 task 1, pressing 'j' moves to column 2 task 2
      3. Assert: after keyboard moves to column 0, mouse click on column 1 updates focus to column 1
    Expected Result: No split-brain between input modalities
    Evidence: Test output
  ```

  **Commit**: YES
  - Message: `feat(ui): kanban board with horizontal columns, task cards, keyboard navigation, and CRUD dialogs`
  - Files: `src/ui.rs`, `src/input.rs`, `src/app.rs`, `src/types.rs`
  - Pre-commit: `cargo test && cargo clippy -- -D warnings`

---

- [ ] 6. OpenCode Integration + Status Detection

  **What to do**:
  - Implement `src/opencode/mod.rs`:
    - `opencode_launch(working_dir) -> Result<()>`:
      - Command: `opencode --cwd <working_dir>` (or just `opencode` if launched from correct dir)
      - This command is sent to the tmux pane via `tmux send-keys`
    - `opencode_resume(working_dir, session_id) -> Result<()>`:
      - Command: `opencode --cwd <working_dir> -s <session_id>`
      - Sent to tmux pane via `tmux send-keys`
    - `opencode_detect_session_id(tmux_session_name) -> Option<String>`:
      - Capture pane content, search for session ID pattern in OpenCode UI
      - Or: Check OpenCode's SQLite DB for recent sessions with matching working_dir
    - `opencode_detect_status(tmux_session_name) -> Status`:
      - Capture last 50 lines of the tmux pane: `tmux_capture_pane(session, 50)`
      - Parse the LAST 30 non-empty lines only (avoid matching code/comments above)
      - Pattern matching (from AoE's proven patterns):
        - **Running**: `"esc to interrupt"` or `"esc interrupt"` in last lines
        - **Waiting**: `"enter to select"`, `"esc to cancel"`, permission prompts (`"Yes, allow once"`, `"Yes, allow always"`)
        - **Idle**: Input prompt present but none of above patterns
        - **Dead**: Tmux pane doesn't exist or process exited
        - **Unknown**: Default/fallback
    - Strip ANSI escape codes before pattern matching (important!)
  - Implement auto-refresh polling loop:
    - Spawn a background tokio task that polls every 3 seconds
    - For each task in DB with an active tmux session: call `opencode_detect_status`
    - Update DB with new status
    - Send status update message to UI event loop
    - Implement backoff: if > 20 tasks, increase polling interval proportionally
    - Implement jitter: randomize polling order to avoid thundering herd
  - Handle the "must target correct pane" requirement: always use `<session>:0.0` (first window, first pane) since we create single-pane sessions

  **Must NOT do**:
  - Don't parse OpenCode's internal SQLite DB (fragile, may change between versions)
  - Don't implement Claude Code, Codex, or other tool detection
  - Don't modify OpenCode's config or behavior
  - Don't implement sound/notification on status change

  **Recommended Agent Profile**:
  - **Category**: `unspecified-high`
    - Reason: Status detection requires careful ANSI parsing, pattern matching, and async polling architecture. Integration testing against real tmux panes.
  - **Skills**: []
  - **Skills Evaluated but Omitted**:
    - `frontend-ui-ux`: Not visual — this is detection logic

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 3 (with Tasks 5, 7)
  - **Blocks**: Task 10
  - **Blocked By**: Tasks 2, 4

  **References**:

  **Pattern References**:
  - `njbrake/agent-of-empires` `src/tmux/status_detection.rs:detect_opencode_status()` — THE reference implementation. Captures last 30 non-empty lines, matches "esc to interrupt" for Running, permission prompts for Waiting, default Idle.
  - `njbrake/agent-of-empires` `src/tmux/session.rs:detect_status()` — `capture_pane(50)` + `get_foreground_pid()` → `detect_status_from_content(content, tool, fg_pid)`
  - `nielsgroen/claude-tmux` `src/detection.rs` — Alternative ANSI-aware detection approach

  **API/Type References**:
  - `tokio::spawn` for background polling task
  - `tokio::sync::mpsc` for sending status updates from poller to UI
  - `std::process::Command` for `tmux capture-pane`

  **External References**:
  - ANSI escape code stripping: `strip-ansi-escapes` crate or manual regex `\x1b\[[0-9;]*m`
  - OpenCode CLI flags: `-s <session_id>` for resume, `--cwd <path>` for working directory

  **WHY Each Reference Matters**:
  - AoE status_detection.rs: This is proven, tested code that handles the exact same problem. Copy the pattern matching logic.
  - AoE session.rs: Shows the pane capture → detection pipeline

  **Acceptance Criteria**:

  **TDD:**
  - [ ] Unit tests for pattern matching: given pane content strings, assert correct status. Use AoE's test fixture approach.
  - [ ] Test ANSI stripping works correctly
  - [ ] Test backoff logic: >20 tasks increases interval
  - [ ] `cargo test opencode` → PASS

  **Agent-Executed QA Scenarios:**

  ```
  Scenario: Status detection identifies running OpenCode
    Tool: Bash (cargo test)
    Preconditions: Status detection module implemented
    Steps:
      1. cargo test test_detect_running_status
      2. Assert: content containing "esc to interrupt" → Status::Running
      3. cargo test test_detect_waiting_status
      4. Assert: content containing "Yes, allow once" → Status::Waiting
      5. cargo test test_detect_idle_status
      6. Assert: content with just input prompt → Status::Idle
    Expected Result: All status patterns detected correctly
    Evidence: Test output

  Scenario: Polling updates task status in DB
    Tool: interactive_bash (tmux)
    Preconditions: App running with at least one task
    Steps:
      1. Start opencode-kanban, create a task
      2. Wait 5 seconds (polling interval)
      3. sqlite3 <db_path> "SELECT tmux_status FROM tasks"
      4. Assert: status is not "unknown" (should be "running" or "idle" depending on opencode state)
    Expected Result: Auto-refresh updates status
    Evidence: sqlite3 query output
  ```

  **Commit**: YES
  - Message: `feat(opencode): launch, resume, and status detection via tmux pane capture`
  - Files: `src/opencode/mod.rs`
  - Pre-commit: `cargo test && cargo clippy -- -D warnings`

---

- [ ] 7. Crash Recovery + Reconciliation

  **What to do**:
  - Implement reconciliation logic in `src/app.rs` (runs on startup):
    1. Load all tasks from SQLite
    2. For each task with a `tmux_session_name`:
       a. Check `tmux_session_exists(name)` → if false, update status to "dead"
       b. If session exists, run `opencode_detect_status()` → update status in DB
    3. Log reconciliation results (tracing)
  - Implement "lazy re-spawn" on attach:
    - When user presses Enter on a "dead" task:
      1. Check if worktree still exists on disk
      2. If worktree exists: create new tmux session in that worktree
      3. If `opencode_session_id` exists in DB: launch `opencode -s <id>` (resume)
      4. If no session ID: launch `opencode` fresh
      5. Update task's `tmux_session_name` in DB
      6. Switch client to the new session
    - When user presses Enter on a task with no tmux session at all (newly created):
      1. Same as above but always fresh launch
  - Handle edge cases:
    - Worktree directory deleted externally: show error dialog, offer to recreate or mark task as broken
    - Repo path no longer exists (unmounted disk): show "repo unavailable" in task card, disable attach
    - Tmux session exists but opencode process is dead: detect via pane capture (no opencode patterns), re-launch opencode in existing session

  **Must NOT do**:
  - Don't auto-recreate sessions on startup (only on user-initiated attach)
  - Don't delete orphaned sessions automatically
  - Don't modify git state during recovery

  **Recommended Agent Profile**:
  - **Category**: `deep`
    - Reason: Reconciliation logic has many edge cases and needs careful state machine design. "Desired state vs observed state" pattern requires deep thinking.
  - **Skills**: []
  - **Skills Evaluated but Omitted**:
    - `ultrabrain`: Edge cases are enumerable, not algorithmically complex

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 3 (with Tasks 5, 6)
  - **Blocks**: Task 10
  - **Blocked By**: Tasks 2, 4

  **References**:

  **Pattern References**:
  - `njbrake/agent-of-empires` README: "Sessions are tmux sessions running in the background. Open and close `aoe` as often as you like. Sessions only get removed when you explicitly delete them." — Same model we're following.
  - tmux-resurrect persistence pattern: Plain text state file with `pane <session> <window_idx> <pane_idx> <path> <command>` format. We do this in SQLite instead.

  **External References**:
  - tmux session lifecycle: `tmux has-session -t <name>` returns 0 if exists, 1 if not

  **WHY Each Reference Matters**:
  - AoE's session model: Validates our "lazy recovery" approach — don't auto-recreate, just detect and offer

  **Acceptance Criteria**:

  **TDD:**
  - [ ] Test: startup with dead sessions → status updated to "dead"
  - [ ] Test: attach to dead task with existing worktree → session recreated
  - [ ] Test: attach to dead task with missing worktree → error shown
  - [ ] `cargo test recovery` → PASS

  **Agent-Executed QA Scenarios:**

  ```
  Scenario: Crash recovery detects dead sessions
    Tool: Bash + interactive_bash
    Preconditions: App running with active task
    Steps:
      1. Create a task, verify tmux session exists
      2. tmux -L opencode-kanban kill-session -t ok-<repo>-<branch> (simulate crash)
      3. Restart opencode-kanban
      4. tmux capture-pane → Assert: task shows dead indicator (✕)
    Expected Result: Dead session detected, task shows dead status
    Evidence: Pane capture showing dead indicator

  Scenario: Attach to dead task re-creates session
    Tool: interactive_bash (tmux)
    Preconditions: App showing dead task
    Steps:
      1. Navigate to dead task
      2. Press Enter
      3. Assert: tmux -L opencode-kanban list-sessions shows re-created session
      4. Assert: opencode process running in the new session
    Expected Result: Session lazily re-spawned
    Evidence: tmux list-sessions + process check
  ```

  **Commit**: YES
  - Message: `feat(recovery): crash detection, lazy re-spawn, and state reconciliation`
  - Files: `src/app.rs` (reconciliation logic)
  - Pre-commit: `cargo test && cargo clippy -- -D warnings`

---

- [ ] 8. Task Creation Flow (Full Pipeline)

  **What to do**:
  - Wire together the full task creation pipeline in `src/app.rs`:
    1. User opens "New Task" dialog (from Task 5)
    2. User selects/adds repo → repo registered in DB (if new)
    3. User enters branch name → validated
    4. User selects base branch (default from repo's detected HEAD)
    5. User enters optional title (defaults to `repo:branch`)
    6. On confirm:
       a. `git_fetch(repo_path)` — fetch latest from origin
       b. `git_create_worktree(repo_path, worktree_path, branch, base_ref)` — create worktree
       c. `tmux_create_session(session_name, worktree_path, None)` — create tmux session
       d. `opencode_launch(worktree_path)` or `opencode -s <id>` — launch opencode in the session
       e. Save all to DB: task record with tmux_session_name, worktree_path
       f. `tmux_switch_client(session_name)` — attach user to the session
    7. On any failure: rollback partial state (kill tmux session if created, remove worktree if created)
  - Handle errors gracefully:
    - Fetch failure: warn but continue (work offline)
    - Worktree creation failure: show error dialog with details
    - Tmux failure: show error dialog
  - Implement "Add new repo" sub-flow:
    - User enters filesystem path
    - Validate it's a git repo (`git_is_valid_repo`)
    - Detect default branch and remote URL
    - Save to DB

  **Must NOT do**:
  - Don't clone repos (user must have them on disk already)
  - Don't auto-push or auto-pull
  - Don't modify existing branches

  **Recommended Agent Profile**:
  - **Category**: `unspecified-high`
    - Reason: This is the integration task — wiring git + tmux + opencode + DB + UI together. Requires careful error handling and rollback logic.
  - **Skills**: []

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 4 (with Task 9)
  - **Blocks**: Task 10
  - **Blocked By**: Tasks 3, 4, 5

  **References**:

  **Pattern References**:
  - `andersonkrs/twig` workflow: `twig tree create <project> <branch>` → creates worktree → copies files → runs post-create → starts tmux session named `{project}__{branch}`. Our pipeline is similar minus the copy/post-create.
  - `njbrake/agent-of-empires` `aoe add . -w feat/my-feature -b` — Creates session on a new git branch. Reference for the add-task pipeline.

  **WHY Each Reference Matters**:
  - twig's tree create: Shows the canonical pipeline order (worktree → session → attach) and error handling
  - AoE's add command: Shows how to integrate worktree creation with session launch

  **Acceptance Criteria**:

  **TDD:**
  - [ ] Test: full pipeline with mock git/tmux (unit test)
  - [ ] Test: rollback on partial failure (worktree created but tmux fails → worktree cleaned up)
  - [ ] `cargo test create_flow` → PASS

  **Agent-Executed QA Scenarios:**

  ```
  Scenario: End-to-end task creation
    Tool: interactive_bash (tmux) + Bash
    Preconditions: Test git repo at /tmp/test-repo with commits
    Steps:
      1. Start opencode-kanban in tmux
      2. Press 'n' to open new task dialog
      3. Enter repo path: /tmp/test-repo
      4. Enter branch name: feature/e2e-test
      5. Accept default base branch
      6. Enter title: "E2E Test Task"
      7. Press Enter to confirm
      8. Assert: switched to new tmux session (verify via tmux display-message)
      9. Detach (Ctrl-B d), switch back to kanban session
      10. Assert: new task visible in TODO column with title "E2E Test Task"
      11. git -C /tmp/test-repo worktree list → Assert: includes new worktree
      12. tmux -L opencode-kanban list-sessions → Assert: includes ok-test-repo-feature-e2e-test
    Expected Result: Complete pipeline: repo → worktree → tmux → opencode → kanban card
    Evidence: tmux captures, git worktree list, tmux list-sessions
  ```

  **Commit**: YES
  - Message: `feat: complete task creation pipeline wiring git, tmux, opencode, and UI`
  - Files: `src/app.rs`
  - Pre-commit: `cargo test && cargo clippy -- -D warnings`

---

- [ ] 9. Configurable Categories + Task Reordering

  **What to do**:
  - Implement category management UI:
    - New keybind: `C` to open category management dialog
    - Dialog allows: add new category, rename existing, delete (only if empty), reorder (keyboard up/down)
    - All dialog items clickable: click category name to select, click action buttons (Add, Rename, Delete)
    - Click outside category dialog dismisses it
    - Categories stored in SQLite `categories` table with `position` field
    - When category deleted: must be empty (no tasks). Show error if tasks exist.
  - Implement task reordering:
    - `J` (Shift+J): Move selected task DOWN in current column
    - `K` (Shift+K): Move selected task UP in current column
    - Update `position` field in DB for all affected tasks
  - Implement task move between columns:
    - `m` then column number/letter: move task to target column
    - Or: popup showing column names, user selects destination
    - Update `category_id` and `position` in DB

  **Must NOT do**:
  - Don't allow deleting the last remaining category
  - Don't allow category names longer than 30 chars
  - Don't implement drag-and-drop for reordering (keyboard J/K only; click to select is fine)

  **Recommended Agent Profile**:
  - **Category**: `quick`
    - Reason: Straightforward DB operations + UI dialogs. Pattern already established in Task 5.
  - **Skills**: [`frontend-ui-ux`]
    - `frontend-ui-ux`: Dialog design for category management needs UX consideration

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 4 (with Task 8)
  - **Blocks**: Task 10
  - **Blocked By**: Task 5

  **References**:

  **Pattern References**:
  - Task 5's dialog implementation — reuse the same modal dialog pattern
  - Task 2's category CRUD functions — already implemented, just wire to UI

  **Acceptance Criteria**:

  **TDD:**
  - [ ] Test: add category → appears in board
  - [ ] Test: reorder tasks updates positions correctly
  - [ ] Test: delete empty category succeeds, delete non-empty fails
  - [ ] `cargo test categories` → PASS

  **Agent-Executed QA Scenarios:**

  ```
  Scenario: Add and rename category
    Tool: interactive_bash (tmux)
    Steps:
      1. Press 'C' to open category manager
      2. Add "REVIEW" category
      3. Assert: 4 columns now visible on board
      4. Rename "REVIEW" to "CODE REVIEW"
      5. Assert: column header updated
    Expected Result: Category CRUD works
    Evidence: Pane captures
  ```

  **Commit**: YES
  - Message: `feat(ui): configurable categories and task reordering`
  - Files: `src/ui.rs`, `src/app.rs`, `src/input.rs`
  - Pre-commit: `cargo test && cargo clippy -- -D warnings`

---

- [ ] 10. Help Panel + Polish + Integration Tests

  **What to do**:
  - Implement help panel overlay:
    - `?` toggles a transparent overlay showing all keybindings + mouse actions
    - Organized by section: Navigation, Task Actions, Category Management, Mouse, General
    - Press `?` again or `Esc` or **click outside** to dismiss
  - Polish and edge cases:
    - Graceful handling of very narrow terminals (show message if too narrow for columns)
    - Empty state: show welcome message with instructions when no tasks exist
    - Loading indicator during git fetch
    - Confirmation before quit if there are active sessions
    - **Mouse polish**: tmux mouse hint (one-time, if no mouse events received after 10s), scroll offset clamping for touchpad bursts, resize invalidates hit-test map
  - Write integration tests that exercise the full pipeline:
    - Create test git repos in temp dirs
    - Run full task creation → status detection → move → delete pipeline
    - Verify SQLite state, tmux state, and git state at each step
  - Final `cargo clippy -- -D warnings` and `cargo fmt` pass
  - Update `Cargo.toml` with correct metadata (description, license, repository URL)

  **Must NOT do**:
  - Don't add README or documentation (out of scope for this task)
  - Don't publish to crates.io
  - Don't add CI/CD

  **Recommended Agent Profile**:
  - **Category**: `unspecified-high`
    - Reason: Integration testing requires orchestrating multiple subsystems. Polish requires attention to edge cases.
  - **Skills**: [`frontend-ui-ux`]
    - `frontend-ui-ux`: Polish (empty states, loading indicators, narrow terminal handling) benefits from UX awareness

  **Parallelization**:
  - **Can Run In Parallel**: NO
  - **Parallel Group**: Wave 4 (final)
  - **Blocks**: None (final task)
  - **Blocked By**: Tasks 5, 6, 7, 8

  **References**:

  **Pattern References**:
  - `nielsgroen/claude-tmux` help panel: `?` keybind shows overlay with all bindings
  - `njbrake/agent-of-empires` `tests/status_detection.rs` — Test fixture approach for status detection

  **Acceptance Criteria**:

  **TDD:**
  - [ ] Integration test: full pipeline (create → detect → move → delete)
  - [ ] `cargo test` → ALL PASS
  - [ ] `cargo clippy -- -D warnings` → clean
  - [ ] `cargo fmt --check` → clean

  **Agent-Executed QA Scenarios:**

  ```
  Scenario: Help panel shows and dismisses
    Tool: interactive_bash (tmux)
    Steps:
      1. Start opencode-kanban
      2. Press '?'
      3. tmux capture-pane → Assert: help overlay visible with keybinding sections
      4. Press '?' again
      5. tmux capture-pane → Assert: help overlay dismissed, board visible
    Expected Result: Help toggle works
    Evidence: Pane captures

  Scenario: Full lifecycle integration test
    Tool: Bash
    Steps:
      1. cargo test integration_test_full_lifecycle
      2. Assert: test creates repo, creates task, verifies worktree, verifies tmux session, moves task, deletes with cleanup
      3. Assert: all state cleaned up after test
    Expected Result: Complete lifecycle verified automatically
    Evidence: Test output

  Scenario: Narrow terminal shows message
    Tool: interactive_bash (tmux)
    Steps:
      1. tmux resize-pane -x 40 -y 10
      2. Start opencode-kanban
      3. tmux capture-pane → Assert: message about minimum terminal size
    Expected Result: Graceful degradation
    Evidence: Pane capture

  Scenario: Help panel lists mouse actions and click-to-dismiss works
    Tool: interactive_bash (tmux)
    Steps:
      1. Start opencode-kanban in tmux session
      2. Press '?'
      3. tmux capture-pane → Assert: help overlay visible with "Mouse" section
      4. Assert: help text mentions "Click" and "Scroll" actions
      5. Press 'Esc' → help dismissed
    Expected Result: Help includes mouse documentation
    Evidence: Pane capture
  ```

  **Commit**: YES
  - Message: `feat: help panel, UI polish, and comprehensive integration tests`
  - Files: `src/ui.rs`, `src/app.rs`, `tests/integration.rs`, `Cargo.toml`
  - Pre-commit: `cargo test && cargo clippy -- -D warnings`

---

## Commit Strategy

| After Task | Message | Key Files | Verification |
|------------|---------|-----------|--------------|
| 1 | `feat: initial project scaffold with ratatui TUI shell and TEA architecture` | Cargo.toml, src/main.rs, src/app.rs | cargo test |
| 2 | `feat(db): SQLite persistence layer with schema, migrations, and CRUD operations` | src/db/mod.rs, src/types.rs | cargo test db |
| 3 | `feat(git): worktree and branch management via git CLI with validation` | src/git/mod.rs | cargo test git |
| 4 | `feat(tmux): session management with dedicated socket isolation` | src/tmux/mod.rs | cargo test tmux |
| 5 | `feat(ui): kanban board with horizontal columns, task cards, keyboard navigation, and CRUD dialogs` | src/ui.rs, src/input.rs, src/app.rs | cargo test ui |
| 6 | `feat(opencode): launch, resume, and status detection via tmux pane capture` | src/opencode/mod.rs | cargo test opencode |
| 7 | `feat(recovery): crash detection, lazy re-spawn, and state reconciliation` | src/app.rs | cargo test recovery |
| 8 | `feat: complete task creation pipeline wiring git, tmux, opencode, and UI` | src/app.rs | cargo test create_flow |
| 9 | `feat(ui): configurable categories and task reordering` | src/ui.rs, src/app.rs | cargo test categories |
| 10 | `feat: help panel, UI polish, and comprehensive integration tests` | src/ui.rs, tests/ | cargo test |

---

## Success Criteria

### Verification Commands
```bash
cargo build --release         # Expected: compiles without errors
cargo test                    # Expected: all tests pass
cargo clippy -- -D warnings   # Expected: no warnings
cargo fmt --check             # Expected: formatted

# Manual smoke test (agent-executable):
tmux new-session -d -s kanban-test
tmux send-keys -t kanban-test "cd /home/cc/codes/opencode-kanban && ./target/release/opencode-kanban" Enter
sleep 2
tmux capture-pane -t kanban-test -p  # Should show kanban board
```

### Final Checklist
- [ ] All "Must Have" features present and working
- [ ] All "Must NOT Have" guardrails respected (no PR creation, no multi-tool, no CLI, etc.)
- [ ] All 10 tasks committed with conventional commit messages
- [ ] All tests pass (`cargo test`)
- [ ] Clean clippy (`cargo clippy -- -D warnings`)
- [ ] Binary runs inside tmux and shows kanban board
- [ ] Can create a task (repo → worktree → tmux → opencode)
- [ ] Can see opencode status indicators on task cards
- [ ] Can move tasks between columns
- [ ] Can delete tasks with cleanup options
- [ ] Survives crash simulation (kill tmux sessions → restart → shows dead → re-attach works)
- [ ] Works on Linux (primary target)
