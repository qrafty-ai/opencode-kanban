# Ratatui Widget Swap Audit Report

**Generated:** 2026-02-16
**Project:** opencode-kanban (feat-uilib branch)
**Target:** Replace custom TUI widget implementations with ecosystem crates

---

## Executive Summary

This audit evaluates the feasibility and risk profile of swapping custom-written TUI widget implementations in the opencode-kanban project with established ecosystem crates. The project currently uses ratatui 0.30 and contains approximately 1,634 lines of custom UI rendering code across 21 render functions in `src/ui.rs`.

**Key Findings:**

- The largest concentration of custom code exists in dialog/menu surfaces (786 LOC) and view/layout components (622 LOC) [Evidence: task-1-ui-inventory.md]
- Nine candidate crates were evaluated for compatibility, maintenance health, and adoption signal [Evidence: task-5-crate-viability.md]
- Risk scores range from 1.70 (low risk) to 4.25 (avoid), with `throbber-widgets-tui` and `tui-popup` being the lowest-risk recommendations [Evidence: task-8-risk-scorecard.md]
- Migration complexity is highest for command palette, new task dialog, board columns, and context menu components [Evidence: task-9-complexity-score.md]
- LOC reduction potential is estimated at 12-20% (high confidence) to 33-50% (aggressive/low confidence) [Evidence: task-7-loc-estimates.md]

**Primary Recommendations:**

1. Adopt `tui-popup` for modal overlay shells (risk: 1.75, LOW)
2. Adopt `tui-prompts` for text input dialogs (risk: 2.00, LOW)
3. Retain custom button/checkbox implementations or adopt `rat-widget` only if full ecosystem is desired
4. Avoid `tui-menu` (archived), `tui-checkbox` (0.30 incompatible), and `tui-widget-list` (maintenance concerns)

---

## Scope

### Scope IN

This audit covers:
- Evaluation of custom TUI widget implementations in `src/ui.rs` (1,634 LOC across 21 render functions)
- Assessment of 9 candidate ecosystem crates for ratatui 0.30 compatibility
- Risk scoring and viability analysis for each candidate crate
- Migration complexity assessment for each component type
- LOC reduction estimates with confidence bands
- Phased rollout recommendations for decision-making purposes

### Scope OUT

This audit explicitly does NOT cover:
- Implementation work (code changes, refactoring, testing)
- Changes to non-UI modules (`src/db/`, `src/git/`, `src/tmux/`, `src/opencode/`)
- Upgrades to ratatui versions beyond 0.30
- Performance benchmarking or runtime measurements
- User acceptance testing or deployment planning
- Integration with other projects or worktrees

---

## 1. Component Inventory

### 1.1 Overview

The opencode-kanban project renders its TUI interface through 21 render functions totaling approximately 1,634 lines of code in `src/ui.rs` [Evidence: task-1-ui-inventory.md].

### 1.2 Component Breakdown

| Component Type | Functions | Approx LOC | % of UI |
|----------------|-----------|-----------|---------|
| View/Layout | 10 | 622 | 38% |
| Dialog/Menu | 4 | 786 | 48% |
| Overlay/Helper | 3 | 71 | 4% |
| Input | 1 | 43 | 3% |
| Button | 1 | 32 | 2% |
| Checkbox | 1 | 33 | 2% |
| Help | 1 | 43 | 3% |

### 1.3 Critical Render Functions

| Function | Line | LOC | Purpose |
|----------|------|-----|---------|
| `render_dialog` | 841-1394 | 554 | Main dialog dispatcher, handles 12+ dialog types |
| `render_columns` | 237-441 | 205 | Kanban columns rendering with tasks |
| `render_command_palette` | 712-839 | 128 | Fuzzy search command palette |
| `render_side_panel_task_list` | 462-600 | 139 | Task list in side panel view |
| `render_side_panel_details` | 602-690 | 89 | Task details panel |

**Source:** `src/ui.rs` lines shown above; `src/app.rs` contains `ActiveDialog` enum at line 195 with 13 variants and dialog-related state structs at lines 49-169.

---

## 2. Component-to-Crate Mapping Matrix

### 2.1 Mapping Overview

This matrix maps each custom component to candidate replacement crates based on functionality overlap.

| Custom Component | Candidate Crate | Evidence Source | Viability |
|-----------------|-----------------|-----------------|-----------|
| Overlay/Modal shells | `tui-popup` | [task-5-crate-viability.md, task-8-risk-scorecard.md] | HIGH |
| Text input fields | `tui-prompts` | [task-5-crate-viability.md, task-8-risk-scorecard.md] | HIGH |
| Scrollable content | `tui-scrollview` | [task-5-crate-viability.md, task-8-risk-scorecard.md] | MEDIUM |
| Checkboxes | (ratatui built-in) | [task-5-crate-viability.md] | N/A |
| Loading indicators | `throbber-widgets-tui` | [task-5-crate-viability.md, task-8-risk-scorecard.md] | HIGH |
| List views | `tui-widget-list` | [task-5-crate-viability.md, task-8-risk-scorecard.md] | LOW |
| Complex forms | `rat-widget` | [task-5-crate-viability.md, task-8-risk-scorecard.md] | HIGH (with lock-in) |

### 2.2 Detailed Mapping

#### 2.2.1 Overlay/Modal Components

| Source Location | Current Implementation | Replacement Candidate |
|-----------------|----------------------|----------------------|
| `src/ui.rs:65` `render_overlay_frame` | Manual Block + Paragraph with BorderType::Double | `tui-popup::Popup` |
| `src/ui.rs:1556` `centered_rect` | Manual percent-based layout calculation | `tui-popup` positioning |
| `src/ui.rs:35` `calculate_overlay_area` | Custom anchor logic (Center/Top) | `tui-popup` placement API |

#### 2.2.2 Input Components

| Source Location | Current Implementation | Replacement Candidate |
|-----------------|----------------------|----------------------|
| `src/ui.rs:1405` `render_input_field` | Custom Block with cursor block glyph `█` | `tui-prompts::TextInput` or `rat-widget::TextInput` |
| `src/ui.rs:1427-1433` | Cursor rendering with inline styled span | Handled by prompt crate |

#### 2.2.3 Interactive Controls

| Source Location | Current Implementation | Replacement Candidate |
|-----------------|----------------------|----------------------|
| `src/ui.rs:1448` `render_button` | Custom Paragraph with focus/hover states | `rat-widget::Button` (optional) |
| `src/ui.rs:1480` `render_checkbox` | Custom with unicode marks `■`/`□` | ratatui built-in Checkbox widget |
| `src/ui.rs:418-438` | Embedded scrollbar in columns | `tui-scrollview` or ratatui Scrollbar |

#### 2.2.4 Dialog Surfaces

| Source Location | Current Implementation | Replacement Candidate |
|-----------------|----------------------|----------------------|
| `src/ui.rs:841` `render_dialog` | 554 LOC dispatcher with 13 branches | `tui-popup` shell + `tui-prompts` inputs |
| `src/ui.rs:903-1028` | NewTask dialog branch | `tui-prompts` form components |
| `src/ui.rs:1029-1108` | DeleteTask dialog branch | Custom retained (simple) |
| `src/ui.rs:1109-1160` | CategoryInput dialog branch | `tui-prompts` input |

---

## 3. Crate Viability Analysis

### 3.1 Summary Matrix

| Crate | Version | License | MSRV | Ratatui Compat | Last Release | Risk Score | Tier |
|-------|---------|---------|------|-----------------|--------------|------------|------|
| throbber-widgets-tui | 0.10.0 | Zlib | 1.86.0 | 0.30 | recent | **1.70** | LOW |
| tui-popup | 0.7.2 | MIT/Apache-2.0 | 1.87.0 | 0.30 | Dec 2025 | **1.75** | LOW |
| tui-prompts | 0.6.1 | MIT/Apache-2.0 | 1.87.0 | 0.30 | Dec 2025 | **2.00** | LOW |
| rat-widget | 3.1.1 | MIT/Apache-2.0 | unknown | latest | Jan 2026 | **2.00** | LOW |
| tui-scrollview | 0.6.2 | MIT/Apache-2.0 | 1.87.0 | ^0.27 | Dec 2025 | **2.10** | MEDIUM |
| tui-widget-list | 0.15.0 | MIT | unknown | unknown | ~2023 | **3.15** | HIGH |
| tui-checkbox | 0.4.1 | MIT | 1.74.0 | 0.29 | Nov 2025 | **3.40** | HIGH |
| tui-menu | 0.3.1 | MIT/Apache-2.0 | unknown | unknown | unknown | **4.25** | AVOID |

**Source:** [task-5-crate-viability.md], [task-8-risk-scorecard.md]

### 3.2 Detailed Viability

#### HIGH Viability (Recommended)

**tui-popup** (risk: 1.75)
- Part of official joshka/tui-widgets workspace
- ~40k monthly downloads
- Compatible with ratatui 0.30
- Production usage: orhun/binsider, orhun/flawz, grouzen/framework-tool-tui
- **Risk:** High MSRV (1.87.0) - requires Rust 2024 edition consideration

**tui-prompts** (risk: 2.00)
- Part of official joshka/tui-widgets workspace
- Compatible with ratatui 0.30
- **Risk:** Limited production evidence, high MSRV (1.87.0)

**throbber-widgets-tui** (risk: 1.70)
- Explicit ratatui 0.30 support
- Production usage: unionlabs/union, Dioxus, matrix-rust-sdk
- **Risk:** Zlib license (compatible but different from MIT), high MSRV (1.86.0)

#### MEDIUM Viability (Needs Verification)

**tui-scrollview** (risk: 2.10)
- Strong production adoption (4+ projects)
- Declares ^0.27.0 - may work with 0.30 but not guaranteed
- **Risk:** Compatibility uncertainty with ratatui 0.30

#### HIGH RISK / AVOID

**tui-checkbox** (risk: 3.40)
- Declares ratatui 0.29 - NOT compatible with ratatui 0.30
- No significant production usage found

**tui-widget-list** (risk: 3.15)
- Stale maintenance (last major activity ~2023)
- Likely incompatible with ratatui 0.30

**tui-menu** (risk: 4.25)
- ARCHIVED/DEPRECATED - Do NOT use

---

## 4. Risk Scorecard

### 4.1 Scoring Methodology

Each factor is scored 1-5 (1=low risk, 5=high risk) with weighted formula:

```
Risk Score = (Compatibility × 0.30) + (Maintenance × 0.25) + (Adoption × 0.25) + (LockIn × 0.20)
```

**Risk Tiers:**
- LOW RISK: 1.0 - 2.0 (Safe to adopt)
- MEDIUM RISK: 2.1 - 3.0 (Caution, monitor)
- HIGH RISK: 3.1 - 4.0 (Significant concerns)
- AVOID: 4.1 - 5.0 (Do not use)

### 4.2 Detailed Scoring

| Crate | Compatibility | Maintenance | Adoption | Lock-in | **Total** | Tier |
|-------|-------------|-------------|----------|---------|-----------|------|
| throbber-widgets-tui | 2 | 1 | 1 | 3 | **1.70** | LOW |
| tui-popup | 2 | 1 | 2 | 2 | **1.75** | LOW |
| tui-prompts | 2 | 1 | 3 | 2 | **2.00** | LOW |
| rat-widget | 1 | 1 | 1 | 5 | **2.00** | LOW |
| tui-scrollview | 3 | 2 | 1 | 2 | **2.10** | MEDIUM |
| tui-widget-list | 4 | 5 | 1 | 2 | **3.15** | HIGH |
| tui-checkbox | 4 | 3 | 4 | 2 | **3.40** | HIGH |
| tui-menu | 5 | 5 | 3 | 2 | **4.25** | AVOID |

**Source:** [task-8-risk-scorecard.md]

### 4.3 Risk Factors Summary

| Risk Type | Affected Crates | Mitigation |
|-----------|-----------------|------------|
| High MSRV (1.86+) | tui-popup, tui-prompts, throbber-widgets-tui | Plan for Rust 2024 edition upgrade |
| Not compatible with 0.30 | tui-checkbox, tui-scrollview | Wait for updates or verify empirically |
| Unmaintained/Archived | tui-menu, tui-widget-list | Avoid entirely |
| Ecosystem lock-in | rat-widget | Only adopt if full rat-* stack needed |

---

## 5. Migration Complexity Analysis

### 5.1 Complexity Scoring Model

Each component scored on four factors (0-3 scale):
- Hit-test coupling: volume of clickable regions and selection geometry
- Message wiring: fanout from UI event to Message to update/side-effect paths
- State shape: amount and mutability of component-specific state
- Edge behavior drift: known brittle behavior (small terminal, unicode, clear/clamp)

**Complexity Tags:**
- Low: 1-3
- Medium: 4-6
- High: 7-10

### 5.2 Component Complexity Matrix

| Component | Hit-test | Message | State | Edge | **Total** | Tag |
|-----------|----------|---------|-------|------|-----------|-----|
| Command palette dialog | 2 | 3 | 2 | 2 | **9** | HIGH |
| New Task dialog | 3 | 3 | 2 | 1 | **9** | HIGH |
| Board columns/task cards | 3 | 3 | 1 | 2 | **9** | HIGH |
| Context menu | 3 | 3 | 1 | 2 | **9** | HIGH |
| Dialog overlay shell | 1 | 3 | 2 | 2 | **8** | HIGH |
| Delete Task dialog | 2 | 3 | 2 | 1 | **8** | HIGH |
| Side panel task list | 2 | 2 | 1 | 1 | **6** | MEDIUM |
| Category Input dialog | 2 | 2 | 1 | 1 | **6** | MEDIUM |
| New Project dialog | 2 | 2 | 1 | 1 | **6** | MEDIUM |
| Worktree Not Found dialog | 2 | 2 | 1 | 1 | **6** | MEDIUM |
| Delete Category dialog | 1 | 2 | 1 | 1 | **5** | MEDIUM |
| Input primitive | 0 | 2 | 0 | 2 | **4** | MEDIUM |
| Repo Unavailable dialog | 1 | 1 | 1 | 1 | **4** | MEDIUM |
| Error dialog | 1 | 1 | 1 | 0 | **3** | LOW |
| Confirm Quit dialog | 1 | 1 | 1 | 0 | **3** | LOW |
| Help overlay | 0 | 1 | 0 | 2 | **3** | LOW |
| Checkbox primitive | 0 | 1 | 0 | 2 | **3** | LOW |
| Button primitive | 0 | 1 | 0 | 1 | **2** | LOW |
| MoveTask placeholder | 0 | 0 | 0 | 1 | **1** | LOW |

**Source:** [task-9-complexity-score.md]

### 5.3 Explicit Behavior Drift Risks

1. **Small terminal behavior**
   - Board view hard-fails at width < `categories * 18` cells (`src/ui.rs:244`)
   - Command palette has viewport gates: width < 30 uses 90% overlay, height < 10 hides results (`src/ui.rs:1396-1403`)
   - Side panel returns early for area.height < 6 (`src/ui.rs:480`)

2. **Glyph rendering**
   - Status glyphs: `●`, `○`, `◐`, `✕`, `!`, `▸`
   - Primitive glyphs: cursor `█`, checkbox marks `■`/`□`, scrollbar `│`, `↑`, `↓`

3. **Overlay clear behavior**
   - `render_overlay_frame` always issues `Clear` before drawing (`src/ui.rs:74`)
   - Critical for avoiding ghosted background content

4. **Context menu clamping**
   - Rendering clamps menu position to viewport bounds (`src/ui.rs:1582-1593`)
   - Mouse handling uses unclamped position - potential coordinate mismatch

### 5.4 High-Complexity Migration Set (Priority Order)

1. Command palette overlay
2. New Task dialog
3. Board columns/task cards
4. Context menu
5. Dialog overlay shell
6. Delete Task dialog

**Source:** [task-9-complexity-score.md]

---

## 6. Ranked Recommendations

### 6.1 Recommended Crate Adoptions

| Rank | Crate | Use Case | Risk | Evidence | Recommendation |
|------|-------|----------|------|----------|----------------|
| 1 | `tui-popup` | Modal/overlay shells | 1.75 | [task-5], [task-6], [task-8] | **ADOPT** - Official ecosystem, low risk |
| 2 | `throbber-widgets-tui` | Loading indicators | 1.70 | [task-5], [task-6], [task-8] | **ADOPT** - Strong adoption, explicit 0.30 |
| 3 | `tui-prompts` | Text input dialogs | 2.00 | [task-5], [task-8] | **ADOPT** - Official ecosystem, monitor adoption |
| 4 | ratatui Checkbox | Checkbox controls | N/A | [task-5] | **USE BUILT-IN** - Already available in ratatui |
| 5 | `tui-scrollview` | Scrollable content | 2.10 | [task-5], [task-6], [task-8] | **VERIFY** - May need version bump for 0.30 |

### 6.2 Adoptions to Avoid

| Crate | Use Case | Risk | Evidence | Recommendation |
|-------|----------|------|----------|----------------|
| `tui-checkbox` | Checkboxes | 3.40 | [task-5], [task-8] | **AVOID** - Not compatible with ratatui 0.30 |
| `tui-widget-list` | List views | 3.15 | [task-5], [task-8] | **AVOID** - Maintenance concerns |
| `tui-menu` | Menus | 4.25 | [task-5], [task-8] | **AVOID** - Archived/deprecated |
| `rat-widget` | Full widget ecosystem | 2.00 | [task-5], [task-6], [task-8] | **OPTIONAL** - Only if full ecosystem needed |

### 6.3 LOC Reduction Estimates

| Confidence Level | Post-Swap LOC | Reduction | Evidence |
|------------------|---------------|-----------|----------|
| High (conservative) | 1,308-1,445 | 189-326 LOC (12-20%) | [task-7-loc-estimates.md] |
| Medium | 1,088-1,308 | 326-546 LOC (20-33%) | [task-7-loc-estimates.md] |
| Low (aggressive) | 811-1,088 | 546-823 LOC (33-50%) | [task-7-loc-estimates.md] |

**Key uncertainty:** `render_dialog` coupling to app message plumbing is the largest driver of LOC reduction uncertainty. Swapping shell widgets does not automatically reduce branch-specific logic [Evidence: task-7-loc-estimates.md].

### 6.4 Decision Matrix by Use Case

| Use Case | Recommended | Risk | Alternative |
|----------|-------------|------|-------------|
| Popup/Modal | `tui-popup` | 1.75 | Manual Block+Paragraph overlay |
| Prompts/Input | `tui-prompts` | 2.00 | tui-textarea, tui-input |
| Scrollable | `tui-scrollview` | 2.10 | ratatui Scrollbar |
| Loading | `throbber-widgets-tui` | 1.70 | Custom char arrays |
| Checkbox | ratatui built-in | - | Custom List-based |
| List Views | (retain custom) | - | ratatui List |
| Complex Forms | (retain custom) | - | tui-textarea + custom |

---

## 7. Migration Decision Framework

> **Note:** This section provides guidance for decision-makers evaluating migration options. It is not an implementation plan. Per Scope OUT, implementation work is explicitly excluded from this audit.

### 7.1 Evaluation Criteria: Phase 1 (Foundation)

**Scope:** Overlay shell infrastructure

| Criterion | Consideration | Risk |
|-----------|---------------|------|
| Component swap | Evaluate `tui-popup` for `render_overlay_frame`, `centered_rect`, `calculate_overlay_area` | LOW (1.75) |
| LOC impact | Estimate 37-49 LOC reduction (overlay helpers bucket) | - |
| Behavioral gates | Small-terminal behavior (< 30 cols, < 10 rows) must remain | Critical |
| Overlay clear | `Clear` before draw must be preserved | Critical |

### 7.2 Evaluation Criteria: Phase 2 (Input Primitives)

**Scope:** Text input dialogs

| Criterion | Consideration | Risk |
|-----------|---------------|------|
| Component swap | Evaluate `tui-prompts::TextInput` for Category Input, New Project dialogs | LOW (2.00) |
| LOC impact | Estimate 15-21 LOC reduction (input field bucket) | - |
| Cursor rendering | Must match current behavior | Verification required |
| Focus/hover states | Must be preserved | Verification required |

### 7.3 Evaluation Criteria: Phase 3 (High-Complexity Dialogs)

**Scope:** Command palette, New Task, Delete Task dialogs

| Criterion | Consideration | Risk |
|-----------|---------------|------|
| Command palette | Complexity 9 - fuzzy ranking, viewport gates | HIGH |
| New Task dialog | Complexity 9 - 7 fields, 2 buttons, checkbox, async state | HIGH |
| Delete Task dialog | Complexity 8 - 3 checkboxes, confirm/cancel wiring | HIGH |
| Rollback approach | Retain current implementation as fallback | Required |

### 7.4 Evaluation Criteria: Phase 4 (Optional)

**Scope:** View/layout refinements

| Criterion | Consideration | Risk |
|-----------|---------------|------|
| tui-scrollview | Evaluate for board columns - compatibility with 0.30 uncertain | MEDIUM |
| throbber-widgets-tui | Evaluate for loading states | LOW |
| ROI assessment | View/layout bucket has lowest reduction potential (5-12%) | - |

### 7.5 Decision Checklist

| Factor | Question | Weight |
|--------|----------|--------|
| Risk tolerance | Are stakeholders comfortable with Phase 1 (LOW) risk level? | Decision |
| Timeline | Is approximate duration acceptable (Phase 1: 1-2 days equiv)? | Decision |
| Behavioral parity | Can acceptance criteria for terminal behavior be defined? | Decision |
| Rollback capability | Can fallback strategy be maintained during migration? | Decision |

> **Summary:** This audit recommends adoption of `tui-popup` and `tui-prompts` at LOW risk (1.75-2.00), with phased evaluation of higher-complexity components. Implementation timeline and acceptance testing criteria should be defined by the project team if migration proceeds.
| 4 | Scroll/Loading | Phases 1-3 pass | Optional - evaluate ROI |

---

## 8. Source References

### Primary Source Files

| File | Lines | Purpose |
|------|-------|---------|
| `src/ui.rs` | 1,669 | Main TUI rendering implementation |
| `src/app.rs` | ~2,100 | State machine, Message handling, dialog state |
| `src/command_palette.rs` | ~200 | Fuzzy ranking logic |

### Evidence Files (Wave 1 Tasks)

| Evidence File | Content |
|--------------|---------|
| `.sisyphus/evidence/task-1-ui-inventory.md` | UI component inventory, function boundaries, LOC breakdown |
| `.sisyphus/evidence/task-2-dialog-coupling.md` | ActiveDialog enum variants, render branch mapping, message wiring |
| `.sisyphus/evidence/task-3-hit-test-map.md` | hit_test_map.push entries, Message variant mapping |
| `.sisyphus/evidence/task-4-command-palette-map.md` | Command palette rendering pipeline, highlight logic, scrolling |
| `.sisyphus/evidence/task-5-crate-viability.md` | Candidate crate research, version compatibility, MSRV |
| `.sisyphus/evidence/task-6-oss-examples.md` | Production OSS usage examples for each candidate crate |
| `.sisyphus/evidence/task-7-loc-estimates.md` | LOC reduction estimates by confidence band |
| `.sisyphus/evidence/task-8-risk-scorecard.md` | Risk scoring methodology and composite scores |
| `.sisyphus/evidence/task-9-complexity-score.md` | Migration complexity by component, drift risk register |
| `.sisyphus/notepads/ratatui-widget-swap/learnings.md` | Key learnings and recommendations summary |

---

## Appendix A: ActiveDialog Variant Reference

Source: `src/app.rs` line 195

| Variant | State Struct | Render Branch | Complexity | Status |
|---------|--------------|---------------|------------|--------|
| None | n/a | Line 1392 | Low | Implemented |
| NewTask | Line 49 | Line 903 | HIGH | Implemented |
| CommandPalette | (external) | Line 895 | HIGH | Implemented |
| NewProject | Line 68 | Line 1306 | MEDIUM | Implemented |
| CategoryInput | Line 130 | Line 1109 | MEDIUM | Implemented |
| DeleteCategory | Line 144 | Line 1161 | MEDIUM | Implemented |
| Error | Line 75 | Line 1286 | LOW | Implemented |
| DeleteTask | Line 95 | Line 1029 | HIGH | Implemented |
| MoveTask | Line 106 | Line 1389 | LOW | Placeholder |
| WorktreeNotFound | Line 159 | Line 1205 | MEDIUM | Implemented |
| RepoUnavailable | Line 166 | Line 1261 | MEDIUM | Implemented |
| ConfirmQuit | Line 81 | Line 1353 | LOW | Implemented (unwired) |
| Help | n/a | Lines 842-845, 1204 | LOW | Implemented |

---

## Appendix B: Message Variants for Hit-Testing

Source: `src/app.rs` Message enum

| Message Variant | Line | Hit-test Entries |
|-----------------|------|-------------------|
| FocusColumn | 226 | 1 |
| SelectTask | 227 | 1 |
| SelectTaskInSidePanel | 228 | 1 |
| DismissDialog | 225 | 9 |
| SubmitCategoryInput | 233 | 1 |
| ConfirmDeleteCategory | 234 | 1 |
| CreateTask | 239 | 1 |
| DeleteTaskToggleKillTmux | 240 | 1 |
| DeleteTaskToggleRemoveWorktree | 241 | 1 |
| DeleteTaskToggleDeleteBranch | 242 | 1 |
| ConfirmDeleteTask | 243 | 1 |
| WorktreeNotFoundRecreate | 244 | 1 |
| WorktreeNotFoundMarkBroken | 245 | 1 |
| RepoUnavailableDismiss | 246 | 1 |
| ConfirmQuit | 247 | 1 |
| CancelQuit | 248 | 1 |
| CreateProject | 257 | 1 |
| FocusNewTaskField | 258 | 4 |
| ToggleNewTaskCheckbox | 259 | 1 |
| FocusCategoryInputField | 260 | 1 |
| FocusNewProjectField | 261 | 1 |

**Total hit-test entries:** 20
**Total unique Message variants used:** 16

---

*End of Audit Report*
