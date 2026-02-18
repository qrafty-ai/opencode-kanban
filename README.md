# opencode-kanban

A Rust terminal kanban board for managing Git worktrees and OpenCode tmux sessions.

## Features

- TUI task board for OpenCode session workflows
- Git worktree management from the board
- tmux-first workflow with quick session switching
- SQLite-backed state and task metadata

## Requirements

- Linux or macOS
- `tmux` installed and available on `PATH`

## Install

### npm

```bash
npm install -g opencode-kanban
```

### Build from source

```bash
cargo build --release
./target/release/opencode-kanban
```

## Run

```bash
opencode-kanban
```

If you run the CLI outside tmux, it creates or attaches to an `opencode-kanban` tmux session automatically.

## Local development

```bash
cargo test
cargo clippy -- -D warnings
cargo build --release
```

## npm release process

Publishing is handled by `.github/workflows/publish-npm.yaml` and uses npm trusted publishing via GitHub OIDC.

1. Create and push a release tag:

```bash
git tag -a v0.1.0 -m "Release v0.1.0"
git push origin v0.1.0
```

For prereleases, use `vX.Y.Z-alpha.N` tags.

The workflow derives the package version from the tag (or manual dispatch input) and updates `Cargo.toml` during CI before building.

The workflow builds platform binaries, packages npm tarballs, and publishes:

- `opencode-kanban` (main package)
- platform-tagged variants for Linux x64, macOS x64, and macOS arm64

## First-time npm setup

In npm package settings, configure a Trusted Publisher for this repository and workflow file:

- Repository: this GitHub repository
- Workflow: `.github/workflows/publish-npm.yaml`

No `NPM_TOKEN` secret is required when trusted publishing is configured.

## License

MIT
