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

- [x] 1. Project Scaffold + TUI Shell
- [x] 2. SQLite Persistence Layer
- [x] 3. Git Worktree Operations Module
- [x] 4. Tmux Session Management Module
- [x] 5. Kanban Board UI + Task CRUD
- [x] 6. OpenCode Integration + Status Detection
- [x] 7. Crash Recovery + Reconciliation
- [x] 8. Task Creation Flow (Full Pipeline)
- [x] 9. Configurable Categories + Task Reordering
- [x] 10. Help Panel + Polish + Integration Tests

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
