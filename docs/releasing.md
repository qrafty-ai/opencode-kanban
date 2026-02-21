# Releasing (Maintainers)

This document covers npm and AUR release automation for `opencode-kanban`.

## npm release process

Publishing is handled by `.github/workflows/publish-npm.yaml` and uses npm trusted publishing via GitHub OIDC.

1. Create and push a release tag:

   ```bash
   git tag -a v0.1.0 -m "Release v0.1.0"
   git push origin v0.1.0
   ```

2. For prereleases, use tags like `vX.Y.Z-alpha.N`.

The workflow derives the build version from the tag and injects it via `OPENCODE_KANBAN_VERSION` during `cargo build`.

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

2. Add the public key (`aur_deploy.pub`) to your AUR account:
   - Log in at <https://aur.archlinux.org/account>
   - Paste the public key into SSH Public Keys

3. Add the private key as a GitHub Actions secret named `AUR_SSH_PRIVATE_KEY`:
   - Repository -> Settings -> Secrets and variables -> Actions -> New repository secret
   - Name: `AUR_SSH_PRIVATE_KEY`
   - Value: base64-encoded private key (GitHub strips trailing newlines from raw PEM secrets):

   ```bash
   cat aur_deploy | base64 -w0
   ```

   Paste the single-line output as the secret value.

4. Claim or create the `opencode-kanban` package on AUR (first publish does this automatically if the package does not exist yet).

## First-time npm setup

In npm package settings, configure a Trusted Publisher for this repository and workflow file:

- Repository: this GitHub repository
- Workflow: `.github/workflows/publish-npm.yaml`

No `NPM_TOKEN` secret is required when trusted publishing is configured.
