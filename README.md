# opencode-kanban

<p align="center">
  <img src="assets/kanban.jpg" alt="opencode-kanban board view" width="49%" />
  <img src="assets/detail.jpg" alt="opencode-kanban detail view" width="49%" />
</p>

A Rust terminal kanban board for managing Git worktrees and OpenCode tmux sessions.

## Why this exists

`opencode-kanban` gives you a single TUI board to track task state while creating and attaching to per-task tmux sessions and Git worktrees.

## Prerequisites

- Linux or macOS
- `tmux` installed and available on `PATH` (required)
- `opencode` installed and available on `PATH` (recommended for attach/resume workflows)
- `sqlite3` available on `PATH` (recommended for OpenCode session lookup)

## Quickstart (2 minutes)

1. Install:

   ```bash
   npm install -g opencode-kanban
   ```

2. Verify runtime tools:

   ```bash
   opencode-kanban --version
   tmux -V
   opencode --version
   sqlite3 --version
   ```

3. Start the app:

   ```bash
   opencode-kanban
   ```

4. In the UI:
   - Press `n` to create a task
   - Press `Enter` on a task to attach
   - Press `?` for built-in help
   - Press `q` to quit

If you start outside tmux, `opencode-kanban` auto-creates or auto-attaches to a tmux session named `opencode-kanban`.

## Installation

### npm

```bash
npm install -g opencode-kanban
```

### AUR (Arch Linux)

```bash
yay -S opencode-kanban
# or
paru -S opencode-kanban
```

### Build from source

```bash
cargo build --release
./target/release/opencode-kanban
```

## First run

- Launch default project:

  ```bash
  opencode-kanban
  ```

- Launch a named project:

  ```bash
  opencode-kanban --project my-project
  ```

- Start with a theme preset:

  ```bash
  opencode-kanban --theme default
  opencode-kanban --theme high-contrast
  opencode-kanban --theme mono
  ```

Each project uses its own SQLite file and board state.

## Core workflows

### Start a new task

1. Press `n` to open the new-task dialog.
2. Pick a repository and enter task details.
3. Press `Enter` to create.
4. Press `Enter` on the task card to attach to its tmux/OpenCode session.

### Resume previous work

1. Open the project (`Ctrl-p` to switch projects if needed).
2. Select the task.
3. Press `Enter` to attach to the existing session.

### Organize work on the board

- Move focus with `h`/`l` and select with `j`/`k`.
- Reorder/move task with `H`/`J`/`K`/`L`.
- Archive selected task with `a`.
- Open archive view with `A`.

## Keybindings cheat sheet

- `Ctrl-p`: switch project
- `n`: new task
- `Enter`: attach selected task
- `h`/`j`/`k`/`l`: navigate board
- `H`/`J`/`K`/`L`: move task
- `a`: archive selected task
- `A`: open archive view
- `?`: help overlay
- `q`: quit

For full, current bindings, use the in-app help overlay (`?`).

## Configuration

- Settings file: `~/.config/opencode-kanban/settings.toml`
- Legacy keybindings file (deprecated): `~/.config/opencode-kanban/keybindings.toml`
- Project databases (Linux default): `~/.local/share/opencode-kanban/*.sqlite`

The app creates config/data files on demand.

## Troubleshooting

- `tmux is required but not available`:
  - Install tmux and confirm `tmux -V` works in the same shell.
- `OpenCode binary not found`:
  - Install OpenCode and confirm `opencode --version` works.
- OpenCode session lookup issues:
  - Install `sqlite3` and confirm `sqlite3 --version` works.
- Mouse scroll/click not working well in tmux:
  - Run `tmux set -g mouse on`.

## Local development

```bash
cargo test
cargo clippy -- -D warnings
cargo build --release
```

## Maintainers

Release and publisher setup docs are in `docs/releasing.md`.

## License

MIT
