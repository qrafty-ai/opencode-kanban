# CLI Integration Plan

## Context

`opencode-kanban` is currently TUI-first and already uses `clap` for runtime options (`--project`, `--theme`) in `src/main.rs`. We want a script-friendly CLI surface that external tools and CI jobs can call without launching the TUI.

Categories are data-driven in storage (`tasks.category_id` foreign key), but parts of the app and draft CLI plan still assume fixed category names (for example, `TODO`). This plan updates the API contract to support fully flexible categories such as `todo`, `in progress`, `review`, and `need rework`.

## Goals

1. Provide scriptable task operations in v1 (`list`, `create`, `move`, `archive`, `show`).
2. Support both local shell usage and CI/CD execution.
3. Offer stable machine-readable output via `--json` with explicit schema versioning.
4. Support flexible category workflows without fixed enum-like column names.
5. Preserve existing TUI behavior for users who run `opencode-kanban` with no subcommand.
6. Return deterministic exit codes for automation.

## Non-Goals (v1)

1. Replacing the TUI as the primary user experience.
2. Adding a network daemon or long-running API server.
3. Solving every tmux/worktree workflow in the first release.

## Locked Product Decisions

1. Priority: scriptable task operations first.
2. Runtime target: local + CI.
3. Output contract: stable JSON schema from day one.
4. Scope: include mutating commands in v1.
5. Command style: noun-verb groups (`task list`, `task create`).
6. Compatibility: SemVer plus explicit JSON schema version.
7. Category identity: `category_id` is canonical; `category_slug` is a stable human-facing selector.

## Core Category Contract

1. `Task` continues to store only `category_id` as the source of truth.
2. `Category` gains or standardizes `slug` as a unique, script-stable identifier.
3. `Category.name` remains user-editable display text.
4. Task write operations accept exactly one destination selector:
   - `category_id`, or
   - `category_slug`.
5. If neither selector is provided for task creation, resolve default category by slug (`todo`) and then by first category position as fallback.

## Proposed CLI Surface

### Global behavior

- `opencode-kanban` with no subcommand launches the TUI (backward compatible).
- `--project <PROJECT>` is available for all CLI paths.
- `--json` switches to machine-readable output.
- `--no-color` and `--quiet` should be supported for cleaner automation logs.

### Command groups

```bash
opencode-kanban category list [--json]
opencode-kanban category create --name <text> [--slug <slug>] [--json]
opencode-kanban category update --id <category-id> [--name <text>] [--slug <slug>] [--position <n>] [--json]
opencode-kanban category delete --id <category-id> [--json]

opencode-kanban task list [--category-id <uuid> | --category-slug <slug>] [--archived <true|false>] [--repo <name>] [--json]
opencode-kanban task create --title <text> [--repo <name>] [--category-id <uuid> | --category-slug <slug>] [--json]
opencode-kanban task move --id <task-id> [--category-id <uuid> | --category-slug <slug>] [--json]
opencode-kanban task archive --id <task-id> [--json]
opencode-kanban task show --id <task-id> [--json]
```

### Output and error contract

- Human mode: concise, grep-friendly lines.
- JSON mode: structured envelope with stable top-level fields.
- Task payloads include both `category_id` and resolved category metadata (`id`, `slug`, `name`, `position`) to avoid extra lookups in scripts.

Example success envelope:

```json
{
  "schema_version": "cli.v1",
  "command": "task show",
  "project": "my-project",
  "data": {
    "task": {
      "id": "task-uuid",
      "title": "Review API",
      "category_id": "cat-uuid",
      "category": {
        "id": "cat-uuid",
        "slug": "review",
        "name": "Review",
        "position": 2
      }
    }
  }
}
```

Example failure envelope:

```json
{
  "schema_version": "cli.v1",
  "error": {
    "code": "CATEGORY_SELECTOR_CONFLICT",
    "message": "Provide exactly one of category_id or category_slug",
    "details": {
      "category_id": "...",
      "category_slug": "..."
    }
  }
}
```

### Exit code policy

1. `0`: success.
2. `2`: invalid CLI usage/arguments.
3. `3`: resource not found (task/project/repo/category).
4. `4`: conflict/invalid selector/state transition.
5. `5`: dependency/runtime error (tmux/git/db IO failures).

## Architecture Plan

### 1) Parsing and dispatch

- Extend `Cli` in `src/main.rs` to include subcommands while keeping current TUI flags.
- Introduce a dispatcher that routes:
  - no subcommand -> existing TUI flow,
  - subcommand -> non-interactive CLI executor.

### 2) New modules

- `src/cli/mod.rs`: top-level command execution.
- `src/cli/commands/task.rs`: handlers for task commands.
- `src/cli/commands/category.rs`: handlers for category commands.
- `src/cli/output.rs`: text/JSON rendering and schema structs.
- `src/cli/errors.rs`: domain-to-exit-code mapping.
- `src/cli/category_resolver.rs`: shared `category_id/category_slug` resolution.

### 3) Data model and migration

1. Add `categories.slug` (unique, non-null) if missing.
2. Backfill slugs for existing rows (`TODO`, `IN PROGRESS`, `DONE` -> `todo`, `in-progress`, `done`).
3. Replace hardcoded default lookup (`category.name == "TODO"`) with slug-first default resolution and position fallback.
4. Keep current `tasks.category_id` storage model unchanged.

### 4) Reusable service layer

- Extract app operations into reusable functions (or thin service structs) so TUI and CLI share logic.
- Reuse existing modules:
  - `src/db/mod.rs` for task/category persistence,
  - `src/git/mod.rs` where task creation needs worktree setup,
  - `src/tmux/mod.rs` for session metadata/actions when needed.

### 5) Safety and idempotency

- `task archive` should be idempotent (archiving an already archived task returns success with a status note).
- `task move` validates destination existence and selector exclusivity.
- `category delete` follows current DB behavior and fails when tasks still reference the category.
- Mutating commands support optional `--dry-run` where behavior is ambiguous/risky.

## Delivery Phases

### Phase 1: Contract and foundations

1. Add subcommand parsing and dispatcher without changing TUI defaults.
2. Add output envelope types and centralized error mapping.
3. Implement `category_id/category_slug` selector parsing and resolver.
4. Implement `task list` and `task show` in text + JSON mode with embedded category metadata.

### Phase 2: V1 mutating commands

1. Implement `task create`, `task move`, and `task archive` using dynamic category selectors.
2. Implement `category list/create/update/delete` commands.
3. Add idempotency and selector validation.

### Phase 3: Hardening and rollout

1. Write CLI integration tests for success/failure and exit codes.
2. Add JSON snapshot-style contract tests to prevent accidental breaking changes.
3. Document CLI usage and automation examples in `README.md`.
4. Ship as a minor release with clear migration notes and deprecation timeline.

## Testing Strategy

1. Unit tests for argument validation and selector exclusivity (`category_id` xor `category_slug`).
2. Unit tests for slug normalization and uniqueness checks.
3. Integration tests for each command in both text and `--json` modes.
4. Integration tests for default category resolution (slug-first, position fallback).
5. Negative-path tests for not-found, selector conflict, dependency failure, and delete-in-use-category.
6. Schema contract tests that assert `schema_version` and required category fields.

## Risks and Mitigations

1. **Risk:** Contract churn breaks integrations.
   **Mitigation:** schema versioning + snapshot tests + release notes.
2. **Risk:** Slug collisions or unstable slug updates break scripts.
   **Mitigation:** unique slug index, explicit slug update path, and deprecation warnings.
3. **Risk:** Divergence between TUI and CLI logic.
   **Mitigation:** shared service layer and avoid duplicated business logic.
4. **Risk:** Side effects in CI (tmux/git assumptions).
   **Mitigation:** make task commands DB-first where possible and gate side effects behind explicit flags.

## Definition of Done (v1)

1. `task list/create/move/archive/show` and `category list/create/update/delete` implemented and documented.
2. `--json` responses stable under `schema_version = "cli.v1"`.
3. Dynamic category selection works via `category_id` and `category_slug`.
4. Hardcoded category-name default (`TODO`) is removed from task creation flow.
5. Exit code mapping is implemented and tested.
6. Existing TUI startup path remains unchanged for no-subcommand usage.
7. CI passes with new CLI tests.
