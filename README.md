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

## AUR release process

Publishing is handled by `.github/workflows/publish-aur.yaml` and targets the [`opencode-kanban`](https://aur.archlinux.org/packages/opencode-kanban) AUR package.

The same version tag that triggers npm publishing also triggers AUR publishing:

```bash
git tag -a v0.1.0 -m "Release v0.1.0"
git push origin v0.1.0
```

The workflow:
1. Strips the `v` prefix to get the semver version
2. Downloads the GitHub release tarball and computes its SHA256
3. Stamps `aur/PKGBUILD` with the version and checksum
4. Pushes the updated PKGBUILD (and auto-generated `.SRCINFO`) to the AUR Git repository

Only stable `vX.Y.Z` tags are supported (no alpha/pre-release variants).

## First-time AUR setup

1. Generate a dedicated SSH key pair (no passphrase):

   ```bash
   ssh-keygen -t ed25519 -C "github-actions@qrafty.ai" -f aur_deploy -N ""
   ```

2. Add the **public key** (`aur_deploy.pub`) to your AUR account:
   - Log in at <https://aur.archlinux.org/account>
   - Paste the public key into **SSH Public Keys**

3. Add the **private key** as a GitHub Actions secret named `AUR_SSH_PRIVATE_KEY`:
   - Repository → Settings → Secrets and variables → Actions → New repository secret
   - Name: `AUR_SSH_PRIVATE_KEY`
   - Value: contents of `aur_deploy` (the private key file)

4. Claim or create the `opencode-kanban` package on AUR (first publish does this automatically if the package doesn't exist yet).

## First-time npm setup

In npm package settings, configure a Trusted Publisher for this repository and workflow file:

- Repository: this GitHub repository
- Workflow: `.github/workflows/publish-npm.yaml`

No `NPM_TOKEN` secret is required when trusted publishing is configured.

## License

MIT
