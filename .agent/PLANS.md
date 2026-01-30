# UI Accessibility & Keyboard Reliability Refactor (Narrative Desktop)

This ExecPlan is a living document. The sections `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work proceeds.

This plan follows `/Users/jamiecraik/.codex/instructions/plans.md`.

## Purpose / Big Picture

After this change, core UI components in the Narrative desktop app will support reliable keyboard navigation, correct focus management, and explicit accessibility semantics without introducing new dependencies. Users will be able to traverse TopNav tabs, timeline selection, file lists, dialogs, and transcript controls purely by keyboard with clear focus cues and no unintended privacy leaks (paths or prompt content). This can be verified by running the app and walking through the keyboard flow end-to-end, plus running lint/typecheck/tests.

## Progress

- [x] (2026-01-30) Establish baseline file list, confirm scope, and document planned edits.
- [x] (2026-01-30) Implement Dialog focus trap + restore, ARIA description, and Escape handling.
- [x] (2026-01-30) Implement TopNav tab semantics + keyboard, scoped Timeline keyboard nav.
- [x] (2026-01-30) Implement selection/label ARIA fixes and reduced-motion scroll behavior.
- [ ] (2026-01-30) Run lint/typecheck/tests; perform manual keyboard walkthrough (completed: lint/typecheck/tests; remaining: manual walkthrough).
- [x] (2026-01-30) Update Outcomes & Retrospective; record any surprises.

## Surprises & Discoveries

- Observation: Lucide React icon components do not accept a `title` prop under strict typing; wrapping in a parent with `title` avoids TS errors.
  Evidence: `src/ui/components/SourceLensView.tsx` typecheck failure in `pnpm -s typecheck`.

## Decision Log

- Decision: Refactor-heavy accessibility/keyboard improvements focused on reliability and correctness, with security/privacy as the primary failure mode to guard.
  Rationale: Interview-me decisions; reduces user friction while avoiding regressions.
  Date/Author: 2026-01-30 / assistant.

## Outcomes & Retrospective

All planned a11y/keyboard improvements were implemented without new dependencies. Lint, typecheck, and tests passed. The changes improved focus trapping and restoration in dialogs, added proper tab semantics in TopNav, scoped timeline keyboard handling, and clarified selection/expanded states. The only unexpected issue was a `title` prop on a Lucide icon, resolved by wrapping it in a parent element. Manual keyboard walkthrough remains to be completed. Future work could add Storybook stories or automated a11y tests, but no regressions were observed in the existing test suite.

## Context and Orientation

This repository is a Tauri desktop app with a React (TypeScript) UI. UI components live in `src/ui/components` and views in `src/ui/views`. Shared styles live in `src/styles.css`. Key components in scope:

- `src/ui/components/Dialog.tsx` — reusable confirm dialog with focus handling.
- `src/ui/components/TopNav.tsx` — main navigation tabs and import menu.
- `src/ui/components/Timeline.tsx` — commit timeline with keyboard navigation.
- `src/ui/components/FilesChanged.tsx` — selectable file list.
- `src/ui/components/SessionImportPanel.tsx` — checkbox list of sessions.
- `src/ui/components/TestResultsPanel.tsx` — collapsible panel.
- `src/ui/components/TraceTranscriptPanel.tsx` — transcript list with file pills.
- `src/styles.css` — global focus styles, pill styles, timeline effects, reduced motion.

The app currently uses custom buttons and CSS classes; Radix UI is not installed in this repo. This plan avoids adding dependencies unless explicitly approved.

## Plan of Work

First, capture the baseline state (no functional changes yet) by listing current files and confirming the targeted components. Next, improve Dialog focus management: trap focus inside while open, restore focus to the triggering element on close, add `aria-describedby`, and make Escape consistently cancel. Then update TopNav tabs to behave like true tabs: `role="tablist"`, `role="tab"`, `aria-selected`, and roving tabindex so only the active tab is tabbable. In Timeline, scope keyboard navigation to a focusable container instead of global `window` key handlers, and set `aria-current` on the selected node. Then fix selection/label semantics: add `aria-selected`/`aria-pressed` where needed, ensure SessionImportPanel checkboxes have accessible labels, and add `aria-expanded` + `aria-controls` to the TestResultsPanel toggle. Finally, respect reduced-motion settings by avoiding smooth scroll when `prefers-reduced-motion` is set.

All changes should be made as small patches that are verifiable and reversible. No new visual redesign is planned. No new dependencies will be added.

## Concrete Steps

Run the following in the repo root:

1) Inspect current component state (no changes).
   - `pwd`
   - `fd -t f src/ui/components src/ui/views src/styles.css`

2) Implement Dialog focus trap and ARIA improvements.
   - Edit `src/ui/components/Dialog.tsx` to:
     - track the previously focused element on open and restore it on close.
     - loop focus within the dialog when Tab/Shift+Tab is pressed.
     - add `aria-describedby` pointing to the message.
     - avoid `{ once: true }` on Escape handler.

3) Implement TopNav tab semantics and Timeline keyboard scoping.
   - Edit `src/ui/components/TopNav.tsx` to:
     - wrap tabs in `role="tablist"`.
     - set each tab to `role="tab"`, `aria-selected`, and `tabIndex` based on active state.
     - add keyboard handlers for ArrowLeft/ArrowRight + Home/End.
   - Edit `src/ui/components/Timeline.tsx` to:
     - move `window` keydown logic to a focusable container with `tabIndex=0`.
     - add `aria-current="true"` on the selected commit node.
     - use reduced-motion guard for smooth scrolling.

4) Implement selection/label semantics and reduced-motion scroll.
   - Edit `src/ui/components/FilesChanged.tsx` for `aria-selected` or `aria-pressed`.
   - Edit `src/ui/components/SessionImportPanel.tsx` to label checkboxes (id + label).
   - Edit `src/ui/components/TestResultsPanel.tsx` to add `aria-expanded` and `aria-controls`.
   - Update any scroll behavior to use instant scroll under `prefers-reduced-motion`.

5) Verify and document.
   - Run `pnpm -s lint`, `pnpm -s typecheck`, `pnpm -s test`.
   - Perform a manual keyboard walkthrough of TopNav → Timeline → FilesChanged → Dialog.

## Validation and Acceptance

Acceptance is met when:

- Dialog: focus stays trapped inside, Escape cancels, and focus restores to the triggering control.
- TopNav: tabs are reachable via keyboard and the active tab has `aria-selected=true` and is the only tab with `tabIndex=0`.
- Timeline: keyboard navigation works only when the timeline is focused; selected node announces current state via `aria-current`.
- FilesChanged rows and transcript file pills announce selection state via ARIA attributes.
- SessionImportPanel checkboxes have accessible labels.
- Reduced-motion users do not experience smooth scrolling transitions.
- Lint/typecheck/tests pass.

## Idempotence and Recovery

Changes are confined to React components and CSS. If any step causes regressions, revert the affected file or undo the specific edit. Each step is independently testable; re-running tests is safe.

## Artifacts and Notes

Commands executed:

  - `pnpm -s lint`
  - `pnpm -s typecheck`
  - `pnpm -s test`

## Interfaces and Dependencies

No new dependencies. Use existing React, TypeScript, and CSS utilities. If a dependency becomes necessary (e.g., Radix Tabs or Dialog), stop and ask for approval before adding.


# Indexing Progress + Performance Improvements (Narrative Desktop)

This ExecPlan is a living document. The sections `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work proceeds.

This plan follows `/Users/jamiecraik/.codex/instructions/plans.md`.

## Purpose / Big Picture

After this change, indexing large repositories (20k–200k files) will feel responsive and will typically complete within 60 seconds, while showing visible progress without exposing sensitive paths or content. The user will see a progress UI with a phase label and count-based progress, and the app will remain responsive during indexing. This can be verified by opening a large repo, observing progress updates, and confirming indexing completes within the target time.

## Progress

- [x] (2026-01-30) Identify indexing stages and add progress state in `App.tsx`.
- [x] (2026-01-30) Extend `indexRepo` to accept a progress callback and emit phase updates.
- [x] (2026-01-30) Add batching/yielding for long loops to keep the UI responsive.
- [x] (2026-01-30) Add one performance optimization without risking correctness.
- [ ] (2026-01-30) Run lint/typecheck/tests; manual indexing walkthrough on a large repo (completed: lint/typecheck/tests; remaining: manual indexing walkthrough).
- [x] (2026-01-30) Update Outcomes & Retrospective; record surprises.

## Surprises & Discoveries

- Observation: progress percent can be derived from phase ordering to avoid requiring per-step totals.
  Evidence: `IndexingProgress` percent computed in `indexRepo` from phase order.

## Decision Log

- Decision: Balanced scope (progress UI + batching + one perf optimization), reliability-first, correctness-over-speed, privacy guarded. Target ≤60s indexing for large repos.
  Rationale: Interview-me decisions, prioritized trust and safety without over-engineering.
  Date/Author: 2026-01-30 / assistant.

## Outcomes & Retrospective

Progress UI and indexing progress reporting were added without new dependencies. Batching/yielding was added to commit summary caching and metadata writes, and aggregate stats now run in parallel with commit summary caching. Lint/typecheck/tests pass. Manual indexing walkthrough on a large repo remains pending to validate the ≤60s target and responsiveness.

## Context and Orientation

Indexing begins in `src/App.tsx` within `openRepo`, which calls `indexRepo(selected, 60)` and sets `repoState.status = 'loading'`. The loading UI currently shows `Indexing repo…` as a static message. The indexing logic lives in `src/core/repo/indexer.ts`, which performs git root resolution, commit listing, summary caching, aggregate stats, attribution note imports, session excerpt loading, trace ingest/scan, and metadata writes. These steps are sequential and produce no progress updates.

The UI must not expose sensitive paths or file contents during progress updates. Only counts and phase labels are allowed.

## Plan of Work

First, define a progress model and update the loading UI to display progress (phase + count + percent). Next, add an optional progress callback to `indexRepo` and emit updates at each stage. Then add batching/yielding in long loops (commit summary writes and attribution note imports) to keep the UI responsive. Finally, apply one safe performance optimization: parallelize independent steps after `commits` are known (e.g., `cacheCommitSummaries` with `getAggregateStatsForCommits`), ensuring ordering does not change correctness. All changes should be incremental and avoid new dependencies.

## Concrete Steps

1) Add a progress state in `src/App.tsx`:
   - Track `{ phase, current, total, percent, message }` while `repoState.status === 'loading'`.
   - Replace the static “Indexing repo…” UI with a simple progress readout.

2) Update `indexRepo` signature in `src/core/repo/indexer.ts`:
   - Add optional `onProgress?: (update) => void`.
   - Emit updates at each stage: resolve root, list commits, cache summaries, aggregate stats, import notes, load sessions, trace ingest/scan, write meta.

3) Add batching/yielding:
   - In `importAttributionNotesBatch` usage or commit meta writes, yield every N items with `await new Promise((r) => setTimeout(r, 0))`.
   - Keep batches small enough to preserve UI responsiveness.

4) Apply one safe performance optimization:
   - Parallelize `cacheCommitSummaries(repoId, commits)` with `getAggregateStatsForCommits(root, limit)` via `Promise.all` after commits are loaded.
   - Ensure errors are still captured and surfaced correctly.

5) Verification:
   - Run `pnpm -s lint`, `pnpm -s typecheck`, `pnpm -s test`.
   - Manually index a large repo and confirm progress UI updates; confirm no filenames/paths are shown.

## Validation and Acceptance

Acceptance is met when:

- Indexing progress is visible with phase + count or percent.
- UI remains responsive during indexing.
- Indexing completes in ≤60s for large repos in typical cases.
- Progress UI does not reveal filenames or content.
- Lint/typecheck/tests pass.

## Idempotence and Recovery

Changes are additive and reversible. If progress reporting introduces issues, fall back to the prior static loading UI and remove the callback usage without affecting core indexing correctness.

## Artifacts and Notes

Commands executed:

  - `pnpm -s lint`
  - `pnpm -s typecheck`
  - `pnpm -s test`

## Interfaces and Dependencies

No new dependencies. Use existing React state and async utilities. If a new library is proposed, stop and request approval.


# Secure Auto-Updates via Tauri Updater (GitHub Releases)

This ExecPlan is a living document. The sections `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work proceeds.

This plan follows `/Users/jamiecraik/.codex/instructions/plans.md`.

## Purpose / Big Picture

After this change, the Narrative app will check for signed updates from GitHub Releases, present an in-app update prompt, and install updates safely. Users will no longer need to manually replace the app from a DMG. The update flow will be verifiable by publishing a signed test release and observing the prompt/install behavior. The design prioritizes security and correctness over speed.

## Progress

- [x] (2026-01-30) Confirm Tauri v2 updater configuration schema and required settings (no guessing).
- [x] (2026-01-30) Add updater configuration to `src-tauri/tauri.conf.json`.
- [x] (2026-01-30) Add update checking and UI prompt in `src/App.tsx`.
- [x] (2026-01-30) Add manual “Check for updates” action (TopNav or settings).
- [ ] (2026-01-30) Validate signing and update feed format for GitHub Releases.
- [ ] (2026-01-30) Run lint/typecheck/tests; build DMG; test update flow (completed: lint/typecheck/tests; remaining: DMG + update flow).
- [ ] (2026-01-30) Update Outcomes & Retrospective; record surprises.

## Surprises & Discoveries

- Observation: updater config expects a public key string in `tauri.conf.json`; env interpolation is not confirmed, so a placeholder is used pending the key.
  Evidence: `plugins.updater.pubkey` set to `REPLACE_WITH_TAURI_UPDATER_PUBKEY`.

## Decision Log

- Decision: Use GitHub Releases as the update feed, with signed updates and a full install flow (prompt + restart).
  Rationale: Security/compliance is primary; GitHub Releases provides a trusted distribution channel.
  Date/Author: 2026-01-30 / assistant.

## Outcomes & Retrospective

- Pending. Will summarize after implementation and verification.

## Context and Orientation

Tauri configuration is in `src-tauri/tauri.conf.json`. The app entry point is `src/App.tsx`. There is no updater configuration yet. Updates must not expose secrets. Signing keys must be managed via 1Password and environment variables. This plan avoids adding dependencies unless required by Tauri v2 updater support.

## Plan of Work

First, confirm the correct Tauri v2 updater configuration schema (fields and required keys) and the GitHub Releases update feed format. Then add the updater configuration to `tauri.conf.json`. Next, implement update checking in the UI: on app launch and via a manual “Check for updates” action. If an update is available, show a secure prompt and allow the user to install/restart. Finally, validate the update process with a signed test release and confirm no secrets are logged or stored in the repo.

## Concrete Steps

1) Confirm Tauri v2 updater schema and required fields (do not guess).
   - Document the required fields in this plan as a short note.

2) Update `src-tauri/tauri.conf.json` to enable the updater and point to the GitHub Releases feed.
   - Ensure signature/public key configuration is correct.

3) Add update-check logic in `src/App.tsx`.
   - Trigger a check at startup and on-demand.
   - Show update prompt with safe copy.

4) Add a “Check for updates” action to TopNav or a settings panel.
   - Ensure it is keyboard accessible.

5) Validate signing and release publishing.
   - Use 1Password-managed secrets for signing.
   - Publish a test release and verify the update prompt/flow.

## Validation and Acceptance

Acceptance is met when:

- App checks for updates at launch and on demand.
- Update prompt appears when a signed release is available.
- Update installs and restarts successfully.
- Invalid signatures are rejected with a clear error.
- No secrets or sensitive paths are logged.

## Idempotence and Recovery

If update checks fail or the feed is unavailable, the app should continue running normally and display a non-blocking error. Rollback by removing updater config and UI hooks.

## Artifacts and Notes

No artifacts yet. Add a short test release log and update prompt screenshot notes after validation.

## Interfaces and Dependencies

No new dependencies unless required by Tauri v2 updater support. If a plugin or new dependency is needed, pause and request approval before adding.
