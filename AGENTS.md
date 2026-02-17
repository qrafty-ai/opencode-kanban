# PROJECT KNOWLEDGE BASE

**Generated:** 2026-02-16
**Commit:** 1c8bef4
**Branch:** feat/boostrap2

## OVERVIEW
Rust TUI kanban board for managing git worktrees and OpenCode sessions, orchestrated via tmux.

## STRUCTURE
```
./
├── Cargo.toml           # edition 2024, dependencies
├── rust-toolchain.toml  # stable channel
├── .pre-commit-config.yaml
├── src/
│   ├── main.rs         # entry, tmux check, terminal setup
│   ├── lib.rs          # re-exports
│   ├── app.rs          # TEA state machine, 2143 lines
│   ├── ui.rs           # ratatui rendering, 793 lines
│   ├── input.rs        # event handling stub
│   ├── types.rs        # Task, Category, Repo structs
│   ├── db/mod.rs       # SQLite CRUD, 720 lines
│   ├── git/mod.rs      # worktree ops, 602 lines
│   ├── tmux/mod.rs     # session mgmt, 552 lines
│   └── opencode/mod.rs # launch/detect, 384 lines
└── tests/
    └── integration.rs  # full lifecycle test
```

## WHERE TO LOOK
| Task | Location | Notes |
|------|----------|-------|
| TUI rendering | src/ui.rs | ratatui widgets, layout |
| App state/TEA | src/app.rs | Message enum, update loop |
| DB schema | src/db/mod.rs | SQLite tables, migrations |
| Tmux commands | src/tmux/mod.rs | Command builders, socket isolation |
| Git worktrees | src/git/mod.rs | create/remove worktree |
| OpenCode integration | src/opencode/mod.rs | launch, status detection |

## CODE MAP
| Symbol | Type | Location | Role |
|--------|------|----------|------|
| `App` | struct | src/app.rs:357 | Main state container |
| `Message` | enum | src/app.rs:159 | TEA messages |
| `Database` | struct | src/db/mod.rs:57 | SQLite wrapper |
| `tmux_switch_client` | fn | src/tmux/mod.rs:76 | Session switch |
| `git_create_worktree` | fn | src/git/mod.rs:86 | Worktree creation |
| `opencode_launch` | fn | src/opencode/mod.rs:50 | OpenCode spawn |

## CONVENTIONS (THIS PROJECT)
- **Never use `sh -c`**: always `Command::new().args([])`
- **HTTP requests**: use `reqwest` (or another request library) instead of manual raw HTTP request construction whenever possible
- **Tmux socket**: use `-L opencode-kanban` for task sessions (default socket for kanban)
- **TDD**: unit tests in `mod tests` blocks, integration in `tests/`
- **No auto-attach**: task creation does NOT switch tmux client; user presses Enter to attach
- **Return to kanban**: `tmux Prefix+K` uses `switch-client -l` (last session)

## ANTI-PATTERNS (THIS PROJECT)
- **Unsafe env**: `env::set_var` requires `unsafe` in multi-threaded (src/opencode/mod.rs)
- **Hardcoded "TODO"**: category name hardcoded in db init (src/db/mod.rs)
- **Process killing**: NEVER kill `opencode` processes broadly - this will kill the agent itself. Use specific PIDs or port-based targeting only when necessary.
- **Expect/unwrap**: used liberally in tmux/git modules - can panic
- **Port 4096 in tests**: integration tests must NOT hardcode port 4096 for mock status servers - use port 0 to bind to an OS-assigned available port. Local OpenCode processes use 4096, causing test failures.

## COMMANDS
```bash
cargo test              # unit + integration
cargo test --lib        # unit tests only
cargo clippy -- -D warnings
cargo build --release
```

## NOTES
- Runs inside tmux only (checks `$TMUX` env var)
- Edition 2024, stable toolchain
- Pre-commit: cargo-check, fmt, clippy (-D warnings)
- Tests use `:memory:` SQLite and isolated tmux socket
