# PROJECT KNOWLEDGE BASE

**Generated:** 2026-03-04
**Commit:** 9c7f528
**Branch:** detached HEAD

## OVERVIEW
Rust terminal kanban for managing project-scoped task boards backed by SQLite, with Git worktrees, tmux sessions, and OpenCode session/status integration. The binary runs in tmux, exposes a TUI plus a small project-scoped CLI, and continuously syncs task runtime state from a local OpenCode server.

## STRUCTURE
```text
./
├── Cargo.toml                 # edition 2024, binary + library crate
├── build.rs                   # injects build version from env/git
├── src/
│   ├── main.rs                # CLI/TUI entry, tmux bootstrap, terminal lifecycle
│   ├── lib.rs                 # broad public module exports
│   ├── app/
│   │   ├── core.rs            # App state container and startup
│   │   ├── messages.rs        # TEA-style message enum
│   │   ├── state.rs           # view/dialog/search state types
│   │   ├── update.rs          # message handling
│   │   ├── navigation.rs      # selection and movement helpers
│   │   ├── polling.rs         # OpenCode status/TODO/message poller
│   │   ├── runtime.rs         # git/tmux abstraction traits
│   │   ├── workflows/         # attach/create/recovery workflows
│   │   ├── actions.rs         # high-level user actions
│   │   └── interaction.rs     # hit testing / interaction map
│   ├── ui.rs                  # ratatui rendering, all views/dialogs
│   ├── cli/mod.rs             # task/category CLI
│   ├── db/mod.rs              # SQLite schema, migrations, CRUD
│   ├── git/mod.rs             # git metadata/worktree helpers
│   ├── tmux/mod.rs            # tmux session/popup/terminal helpers
│   ├── opencode/
│   │   ├── mod.rs             # launch/attach/binding helpers
│   │   ├── server.rs          # local server bootstrap/readiness
│   │   └── status_server.rs   # async HTTP client/parsing
│   ├── projects.rs            # project DB file discovery/lifecycle
│   ├── settings.rs            # settings load/save/validate
│   ├── theme.rs               # theme presets and custom theme resolution
│   ├── keybindings.rs         # keymap defaults and overrides
│   ├── command_palette.rs     # command palette ranking/state
│   ├── task_palette.rs        # cross-project task search/ranking
│   ├── matching.rs            # fuzzy matching helpers
│   ├── notification/mod.rs    # tmux/system notifications
│   ├── realm.rs               # tui-realm adapter
│   └── types.rs               # Repo/Category/Task/session DTOs
└── tests/
    └── integration.rs         # git/tmux/opencode lifecycle coverage
```

## WHERE TO LOOK
| Task | Location | Notes |
|------|----------|-------|
| App startup/state | `src/app/core.rs` | `App`, startup reconciliation, poller boot |
| Message/update flow | `src/app/messages.rs`, `src/app/update.rs` | TEA-style event handling |
| UI rendering | `src/ui.rs` | project list, board, archive, settings, dialogs |
| Dialog/view state | `src/app/state.rs` | top-level view and form state enums |
| Task creation/attach | `src/app/workflows/` | repo/worktree/session orchestration |
| DB schema and migrations | `src/db/mod.rs` | tables, backfills, sync/async bridge |
| Git worktree behavior | `src/git/mod.rs` | branch detection, worktree create/remove, diff summary |
| tmux integration | `src/tmux/mod.rs` | session lifecycle, popup text, terminal launcher |
| OpenCode integration | `src/opencode/` | launch/attach, server bootstrap, HTTP polling |
| Project DB files | `src/projects.rs` | one SQLite file per project |
| Settings/themes/keys | `src/settings.rs`, `src/theme.rs`, `src/keybindings.rs` | persisted customization |
| Search palettes | `src/command_palette.rs`, `src/task_palette.rs` | fuzzy ranking and usage history |

## CODE MAP
| Symbol | Type | Location | Role |
|--------|------|----------|------|
| `App` | struct | `src/app/core.rs:20` | Main interactive state container |
| `Message` | enum | `src/app/messages.rs:13` | TUI/application event surface |
| `View` | enum | `src/app/state.rs:256` | Top-level TUI view selection |
| `ViewMode` | enum | `src/app/state.rs:334` | Kanban vs side-panel mode |
| `Database` | struct | `src/db/mod.rs:26` | SQLite wrapper and migration entry |
| `git_create_worktree` | fn | `src/git/mod.rs:144` | Worktree creation |
| `tmux_switch_client` | fn | `src/tmux/mod.rs:85` | Attach current client to task session |
| `opencode_launch` | fn | `src/opencode/mod.rs:77` | Launch/resume OpenCode shell command |
| `Settings` | struct | `src/settings.rs:31` | Persisted user config |
| `spawn_status_poller` | fn | `src/app/polling.rs:25` | Background OpenCode synchronization |

## CONVENTIONS (THIS PROJECT)
- **Never use `sh -c`**: spawn processes with `Command::new(...).args([...])`.
- **HTTP should use `reqwest`**: both blocking and async clients already exist in the codebase.
- **tmux socket selection is env-driven**: prefer `OPENCODE_KANBAN_TMUX_SOCKET`; tests rely on isolated sockets.
- **Task creation is non-attaching**: creating a task/session does not switch the active tmux client; attach is a separate action.
- **Project scope is a SQLite file**: each project is its own `.sqlite` DB under the local data dir.
- **Board state lives in SQLite**: repos, categories, tasks, archive state, session metadata, and command frequency are persisted there.
- **Settings live in TOML**: user customization belongs in `settings.toml`, not ad hoc files.
- **TDD split**: unit tests live in module `mod tests`; cross-subsystem behavior goes in `tests/integration.rs`.
- **Use runtime traits for workflows**: `RecoveryRuntime` and `CreateTaskRuntime` are the test seam for git/tmux-heavy logic.
- **Return to kanban uses tmux last-session behavior**: helper text and bindings assume `Prefix+K` maps to `switch-client -l`.

## ANTI-PATTERNS (THIS PROJECT)
- **Do not trust the old flat app layout**: `src/app.rs` no longer exists; the app is split across `src/app/`.
- **Do not kill `opencode` broadly**: the agent may be running inside it. Target specific PIDs or ports only when necessary.
- **Do not hardcode port `4096` in tests for mock servers**: runtime defaults use `4096`; tests should bind port `0` to avoid collisions.
- **Avoid expanding the sync/async DB bridge casually**: `block_on_db` exists for compatibility, but adding more blocking wrappers increases coupling.
- **Hardcoded `TODO` semantics exist**: default category seeding still bakes in `TODO`, `IN PROGRESS`, and `DONE`.
- **Production `unsafe` is not expected**: current `unsafe` env mutation is confined to tests because Rust 2024 makes process env writes unsafe.
- **The library surface is broad**: `src/lib.rs` exports most modules. Treat public visibility as intentional only when needed.
- **Matcher panics are masked**: fuzzy matching is wrapped defensively, so bad matcher behavior may degrade ranking instead of crashing loudly.

## COMMANDS
```bash
cargo test              # unit + integration
cargo test --lib        # unit tests only
cargo clippy -- -D warnings
cargo build --release
```

## NOTES
- Runs under Tokio via `#[tokio::main]`.
- Auto-bootstrap enters tmux when `$TMUX` is not set.
- Default OpenCode server URL is `http://127.0.0.1:4096`.
- `src/ui.rs` is large and still monolithic; most UI changes require careful regression checking.
- Reference docs generated from the current codebase:
  - `FUNCTIONALITY.md`
  - `DESIGN_KNOWLEDGE.md`
  - `ENGINEERING_KNOWLEDGE.md`
  - `REWRITE_SPEC.md`
