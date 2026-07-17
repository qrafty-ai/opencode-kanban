## Context

The ordinary New Task dialog already collects a new local branch name and a free-text base ref, then creates a worktree with a new branch. Although the Git layer can enumerate local and remote branches, the dialog does not expose that information. A remote-tracking branch typed as the base is also not intentionally configured as the new branch's upstream.

This change keeps the existing task model: every task owns a newly created local branch and worktree. It improves selection of the branch that the local branch starts from.

## Goals / Non-Goals

**Goals:**
- Add a discoverable source-branch control to the existing New Task dialog.
- Support local and `origin/*` remote-tracking branches as sources.
- Configure an explicitly selected remote source as the upstream of the created local branch.
- Retain a typed-ref path for refs not offered by the picker.
- Provide actionable feedback when fetching or resolving a source fails.

**Non-Goals:**
- Adding a separate task type, pull-request mode, provider integration, or PR-ref fetching.
- Changing task persistence, worktree ownership, or attachment behavior.
- Supporting remotes other than `origin` in the initial picker and automatic tracking flow.

## Decisions

### Evolve Base into a source-branch picker with text fallback

The existing **Base** field will become **From** and remain editable. Its picker will reuse the dialog's existing fuzzy-picker interaction pattern and show local and remote branches as distinct result groups. This adds discoverability without removing support for arbitrary Git refs.

An additional task workflow or source-mode selector was rejected because the source itself is sufficient to determine behavior and users requested that this remain part of ordinary task creation.

### Treat remote selection as an explicit tracking request

When a user selects an `origin/<branch>` entry, task creation will create the requested local branch at that ref and configure its upstream to the selected `origin/<branch>`. The tracking relationship is explicit rather than relying on Git's version- and command-dependent auto-tracking heuristics.

When the source is local or an arbitrary typed ref, creation retains current behavior and does not invent an upstream relationship.

### Refresh origin before remote source resolution

The dialog/workflow will refresh `origin` before the remote list is used for task creation, then resolve the selected source from the refreshed remote-tracking refs. Fetch failures and absent selected refs stop creation with a user-visible error rather than silently falling back to stale or missing data.

The existing optional base-freshness check will be aligned with the renamed source field and continue to apply only where an `origin` remote-tracking comparison is meaningful.

### Preserve user control over the local branch name

The Branch field remains the target local branch name. Selecting a remote source can prefill it with the remote branch's short name only while it has not been edited; the user can always choose a different target name. The selected remote remains the upstream even if the local name differs.

## Risks / Trade-offs

- [A remote branch disappears or is rewritten between selection and creation] → Refresh and resolve it immediately before worktree creation; report the failed source.
- [A remote short name collides with an existing local branch] → Retain existing duplicate-branch error handling and require a different target name.
- [The branch list is large] → Use the existing fuzzy picker rather than rendering an unfiltered list in the compact dialog.
- [Typed refs cannot be classified reliably] → Only auto-track explicit picker selections (or unambiguous `origin/*` entries); retain normal base-ref behavior otherwise.
