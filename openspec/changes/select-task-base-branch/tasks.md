## 1. Source-branch state and Git support

- [x] 1.1 Extend New Task state and messages to retain the source ref and whether it was selected as a remote branch.
- [x] 1.2 Add Git helpers to refresh `origin`, enumerate normalized local and `origin/*` source choices, resolve a selected remote ref, and set a new branch's upstream explicitly.
- [x] 1.3 Update worktree creation to apply upstream tracking only for an explicitly selected remote source and return actionable fetch, resolution, and tracking errors.

## 2. Ordinary task dialog

- [x] 2.1 Replace the Base presentation with an editable From source-branch control in the existing New Task dialog.
- [x] 2.2 Reuse the existing fuzzy-picker behavior to search and select grouped local and remote branch choices while preserving manual ref entry.
- [x] 2.3 Fetch and refresh remote choices at the appropriate dialog interaction point, display failures, and preserve Tab/arrow navigation.
- [x] 2.4 Prefill the Branch target from a selected remote branch only when the user has not already edited the target name.

## 3. Verification

- [x] 3.1 Add Git-layer tests for creating from a local branch, creating from a remote branch, explicit upstream configuration, unavailable remote refs, and fetch failures.
- [x] 3.2 Add dialog/workflow tests for source selection, manual source refs, branch-name prefilling, and creation error presentation.
- [x] 3.3 Update or add integration coverage for creating an ordinary task from an `origin/*` branch and confirming its worktree and upstream configuration.
- [x] 3.4 Run the focused tests and the relevant Cargo test suite.
