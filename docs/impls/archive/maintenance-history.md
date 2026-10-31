# Maintenance history

Task history and actual outcomes extracted on 2026-10-22 before deleting small-change plans. These are historical records, with their original results and uncertainty; they are not current instructions or new validation. Read only the section needed for a particular past event. The [archive policy](../../README.md#archive-policy) governs retention.

## Initial MVP landings

Source: [original record](https://github.com/yicheng47/runner/blob/8462b5194e0f1945ef62b1af1a2758c6816fd70d/docs/impls/archive/0001-v0-mvp.md).

| Area | PR | What is live |
|------|----|--------------|
| C1 Schema + shared types | #4 | SQLite schema, Rust/TS domain types, default signal-type allowlist seeded per crew. |
| C2 Config commands | #7 | Global runner CRUD, crew CRUD, and the original crew membership commands. PR #24 supersedes membership storage with `slots` and moves the lead invariant fully into Rust transactions. |
| C3 Config UI | #9 | Crews page, Crew Detail, Add Slot modal, runner edit drawer; PR #24 updates these surfaces to pick existing runner templates into slots. |
| C4 Event log | #10 | `runner-core` event-log primitives: `flock`-scoped NDJSON append, monotonic ULIDs with an on-disk floor, crash-tail repair, lossy reads for malformed tails. |
| C5 Mission lifecycle | #11 | Transactional `mission_start` / `mission_stop`, one-live-mission-per-crew invariant, opening/terminal log events, atomic `signal_types.json` sidecar. |
| C5.5a Shared runners | #13 | Runners became top-level reusable templates and `sessions.mission_id` became nullable for direct-chat sessions. PR #24 replaces the intermediate `crew_runners` join with `slots`. |
| C6 PTY runtime | #12 | `portable-pty` session manager, reader threads, stdin injection, kill/reap semantics, all-or-nothing mission spawn rollback, `$APPDATA/runner/bin` on child `PATH`. |
| C7 Event bus | #14 | `notify` watcher per live mission, replay-on-mount, messages-only inbox projection, `inbox_read` watermarks, `event/appended`, `inbox/updated`, `watermark/advanced` events. |
| C8.5 Runner surfaces | #15 | `/runners`, `/runners/:handle`, direct-chat session backend, `runner_list_with_activity`, `runner/activity` live counters. |
| Rename / namespace cleanup | #16 + follow-up | Project/crate/app namespace is singular `runner`; env vars are `RUNNER_*`; app data is under `$APPDATA/runner`; planned CLI binary is `runner`. SQL table names stay plural where they represent row collections. |
| Direct-chat frontend hardening | #17 | xterm.js direct-chat pane, persistent sidebar SESSION list, PTY resize handshake, base64 raw PTY output for TUI fidelity, reload reattach, and listener-before-spawn ordering for direct sessions. |
| C8 Signal router v0 + runtime adapter | #18 | Flat parent-process dispatcher (`src-tauri/src/router/`): handlers for `mission_goal`, `human_said`, `ask_lead`, `ask_human`, `human_response`, `runner_status`. Pending-ask + status maps reconstruct on reopen via a replay high-water ULID; live tail short-circuits at-or-below the watermark. Runtime adapter wires `runner.system_prompt` into both mission and direct-chat spawn paths: `claude-code` uses `--append-system-prompt`; Codex has no verified system flag, so fresh sessions receive it as the positional initial prompt and resume must not replay it. |
| C9 `runner` CLI binary | #19 | New `cli/` workspace member produces `runner-cli`, installed at app startup as `$APPDATA/runner/bin/runner` (rename on copy). Verbs: `signal`, `msg post`, `msg read`, `status`, `help`. Validates against per-crew `signal_types.json` and per-mission `roster.json` sidecars (frozen at mission_start). `inbox_read` is suppressed on `--from` filtered reads to avoid corrupting the global per-runner watermark. Tauri's `beforeDev`/`beforeBuild` build the CLI alongside the app so the dev install path needs no manual cargo step. |
| C10 Mission workspace UI | #20 | `/missions/:id` page (`MissionWorkspace.tsx`) with header status pill + Stop, tabbed center column (Feed / per-slot PTY), MissionInput dock for `human_said` signals (default recipient `@<lead>`, broadcast via × icon), AskHumanCard with attribution chain + `human_response` submission, RunnersRail with PTY status + `runner_status` badge + LEAD flag. Backend additions: `mission_events_replay` (lossy log read), `mission_post_human_signal` (human-only allowlist for `human_said` / `human_response`), `session_output_snapshot` (bounded ring buffer per session, replayed on late attach), `OutputEvent.seq` for snapshot-vs-live merge, `SessionRow.lead` denormalized through `slots`. RunnerChat refactored onto the same per-session-pane stack so direct chats survive switching. Tauri config now sets `acceptFirstMouse: true` so the dev window's first click reaches UI elements instead of being eaten by the macOS focus transition. |
| Crew slots / per-slot identity | #24 | `slots` replaces `crew_runners`; `runners.role` is dropped; mission sessions store `slot_id`; `slot_handle` is the in-mission identity used by `RUNNER_HANDLE`, event senders, router routing, roster sidecars, workspace tabs, and Crew Detail. The same runner template can fill multiple slots in one crew with distinct handles. |
| C11 Missions list + Start Mission modal | #21 | `/missions` page (`Missions.tsx`) with Active/Past tabs over `mission_list_summary` (joins each row with crew name + live `pending_ask_count` from `RouterRegistry`, falls back to a log scan of unmatched `human_question`/`human_response` pairs for unmounted or orphan-running missions). Mission rows surface a warn-tinted `N pending` flag, click-through to the workspace. `StartMissionModal.tsx` provides the crew picker (warns when the chosen crew has no runners), title, goal textarea (placeholder defaults from `crew.goal`), cwd input + `Browse…` via `@tauri-apps/plugin-dialog`, and a stubbed Advanced disclosure. Backend: `Router::pending_ask_count()`. Sidebar Mission link enabled. |

## Pane-fill investigation and terminology correction

Source: [original record](https://github.com/yicheng47/runner/blob/8462b5194e0f1945ef62b1af1a2758c6816fd70d/docs/impls/archive/0022-new-chat-pane-fill-window-ownership.md).

### Status

Investigation + fix design for a multi-window dogfooding bug in the direct-chat split view (impl [0020](https://github.com/yicheng47/runner/blob/8462b5194e0f1945ef62b1af1a2758c6816fd70d/docs/impls/archive/0020-direct-chat-split-view.md), built on multi-window impl [0018](https://github.com/yicheng47/runner/blob/8462b5194e0f1945ef62b1af1a2758c6816fd70d/docs/impls/archive/0018-multi-window.md)). This doc is the impl log; the fix itself is a small, localized frontend change (see [Fix](#fix)). Branch: `fix/new-chat-window-ownership` (rebased on `origin/main` @ `39c7587`, which already carries the group-pin-inheritance work).

### Problem (user repro, deterministic — "happens each time")

With more than one Runner window open, in a split tab (chat group in this doc's original vocabulary — see the terminology follow-up at the bottom):

1. Archive one pane's chat — the pane empties in place, the other pane keeps its chat.
2. Hit **New chat**.
3. The newly-created session does **not** land in the current window's empty pane. The split silently collapses to a single full-screen chat, and the work the user was in "ends up belonging to a separate window."

Expected: the new session fills the emptied pane, the split stays intact, and the acting window keeps owning every chat it shows.

### Root cause

**The sidebar new-chat entry point navigates to the spawned session without making it a member of the active group.** It is the only new-session path that skips the group.

There are three ways a fresh or existing chat is placed into a split, and the design invariant (`RunnerChat.tsx:926-931`) names exactly three explicit membership-add acts — the preset picker, the empty-pane `StartChatModal`, and a sidebar pick into a focused empty pane. Two of the three assign into a pane; the sidebar **new-chat** path does not:

| Entry point | Handler | Assigns into a pane? |
|---|---|---|
| Empty-pane "New chat" button (path a) | `RunnerChat.tsx:1769` `StartChatModal.onStarted` | ✅ `assignSessionToPane(target, spawned.id)` then `focusPane` |
| Sidebar row **pick** into empty pane | `Sidebar.tsx:799-845` `openDirectChat` | ✅ `assignSessionToPane(focusedPaneId, …)` when the focused pane is empty |
| **Sidebar "new chat" row + ⌘T (path b)** | `Sidebar.tsx:1090-1098` `StartChatModal.onStarted` | ❌ **only `navigate(\`/chats/${spawned.id}\`)`** |

`⌘T` (`Sidebar.tsx:406-410`) and the "new chat" nav row (`Sidebar.tsx:924`, `:983`) both route to the same `creatingChat` modal, whose `onStarted` only navigates. So the most natural "hit New chat" gesture is the one broken path.

#### Why that produces the collapse and the "separate window" symptom

The split is a *chat group* — a binding between specific sessions, not a viewport mode. `RunnerChat` renders the group only while the open chat is a member:

```
// RunnerChat.tsx:294
const splitActive = isGroupActiveFor(layout, sessionId);   // false for a non-member
```

Because path (b) never adds `spawned.id` to the layout, `isGroupActiveFor(layout, spawned.id)` is **false**, so the group backgrounds and the new chat renders single-pane. The emptied pane is abandoned — that is the visible "the new chat didn't land in the pane" half.

The multi-window half follows from how a window's *ownership* is derived. Each window reports the subjects it shows via `useReportSubjects` (`RunnerChat.tsx:293-320`), and that list is computed from `splitActive`:

```
// RunnerChat.tsx:313-317
const paneSessionIds = splitActive ? visibleSessionIds(layout.root) : [];
const subjectIds =
  sessionId && !paneSessionIds.includes(sessionId)
    ? [sessionId, ...paneSessionIds]
    : paneSessionIds;
```

Before the new chat, the acting window reported every pane's session (e.g. `[X, Y]`, then `[X]` after archiving `Y`). The instant path (b) backgrounds the group, `splitActive` flips to `false`, `paneSessionIds` becomes `[]`, and the window reports **only the new session** — it *drops* the subjects for the group members it was still showing. In the backend `WindowRegistry` the most-recently-focused holder of a subject is its owner (`windows.rs:127` `primary_for`; mirrored frontend by `isSecondaryFor`, `windowFocus.ts:143`). The natural reason to have two windows open is that one of the split's chats is also open in the other window; the acting window was that chat's owner (it was focused last). When path (b) makes the acting window stop reporting it, the *other* window becomes the sole/primary holder and its `DuplicateSubjectOverlay` clears — the chat the user was driving "jumps" to the separate window, while the new chat is stranded solo.

#### What is *not* the root cause (verified, so the fix stays localized)

- **The arbitration is sound.** `WindowRegistry` (`windows.rs`) and `isSecondaryFor` (`windowFocus.ts:143-166`) resolve ownership by `focused_at` correctly; the existing unit tests cover the tie/promotion/hidden-main cases. No registry change is needed, and the fix must **not** touch the overlay.
- **No window ever *adopts the new session*.** A window's registry subjects are set exclusively by its own `window_report_subjects` call, driven only by its own URL + layout store. Nothing — not the backend spawn (`commands/session.rs:510` `session_start_direct` never touches `state.windows`), not any `session/*` event listener, not the shared-but-only-`main`-persisted layout store (`paneLayout.ts:400-410`) — causes a *second* window to navigate to or assign the new session. So the new session is always reported by the acting window alone, which is focused, so it is always the new session's owner; the overlay can never fire on the *new* session itself. The observed "belongs to a separate window" is the *group members'* ownership migrating away, per the mechanism above — a refinement of the initial hypothesis that framed the overlay as landing on the new session.
- **The debounce (`windowFocus.ts:77-91`) is not the trigger.** It coalesces last-write-wins per window; the acting window's final reported set is correct. The collapse is a pure membership-logic outcome, which is why it is deterministic ("happens each time") rather than timing-dependent.

#### One detail the fix must respect: the emptied pane is *not* focused

`archiveSession` (`RunnerChat.tsx:1217-1271`) leaves focus on a live pane in both cases — archiving a background pane keeps focus on the URL-owner (no navigation), and archiving the URL-owner focuses the *survivor* and navigates to it (`:1244-1253`). Either way the URL sits on a live member and the emptied pane is left **unfocused**. So a fix that fills only the *focused* empty pane (as `openDirectChat` does at `:822`) would not fire for this repro — the fix must target the first empty pane in the active group, matching what the empty-pane button already does (it targets a specific, possibly-unfocused pane, `ChatPaneGroup.tsx:250` → `RunnerChat.tsx:1730`).

### Fix

Make the sidebar new-chat `onStarted` fill the active group's empty pane, mirroring path (a) and the pick path. Concretely, in `Sidebar.tsx` `StartChatModal.onStarted` (`:1093`):

1. Read the store layout (`getPaneLayout()`).
2. If the current chat's group is active (`isGroupActiveFor(layout, currentChatSessionId)`) and has an empty pane, `assignSessionToPane(firstEmptyPaneId, spawned.id)` + `focusPane(firstEmptyPaneId)`, and inherit the group pin the same way `openDirectChat` does (`shouldInheritPinOnAdd` / `api.session.pin`), so the new member stays in the group's sidebar cluster.
3. Navigate as today.

When no group is active or there is no empty pane, behavior is unchanged (plain navigate to a single-pane chat).

Because the new chat becomes a member, `splitActive` stays `true`, the split keeps rendering, the emptied pane is filled, and the acting window keeps reporting `[…, spawned.id]` for every pane — so it retains ownership of all of them and no other window adopts or is handed anything. This addresses the root cause (group membership), not the symptom (it does not suppress the overlay).

#### Extract a pure helper for testability

The pane-fill decision is pure and belongs in `paneLayout.ts` next to the existing helpers, so it can be unit-tested in the `paneLayout.test.ts` style without React:

```ts
/** The pane a new chat should fill: the first empty pane of the active
 *  group for `currentChatSessionId`, or null when there is no active
 *  group / no empty pane (caller falls back to plain navigation). */
export function newChatTargetPane(
  layout: PaneLayout,
  currentChatSessionId: string | null,
): string | null
```

Scope `newChatTargetPane` to the **sidebar new-chat path only** — it is the path with no specific pane in hand, so "first empty pane of the active group" is the right rule for it. Do **not** route the empty-pane button through it: that path already carries an explicit clicked-pane id (`ChatPaneGroup.tsx:249-250` → `RunnerChat.tsx:1769-1777`), and collapsing it to "first empty pane" would misfire in a 3-pane layout with two empty panes (clicking the *second* empty pane would fill the *first*). The empty-pane button and the pick path stay as they are; only the sidebar new-chat path gains the helper. Keep the change minimal and localized — no registry or pane-layout refactor beyond this helper — so it merges cleanly with any concurrent split-view work.

### Verification

- **New unit tests** (`paneLayout.test.ts`) for `newChatTargetPane`: returns the emptied pane after a background-pane archive; returns the emptied pane after a URL-owner archive (survivor focused, empty pane unfocused); returns `null` for a single-pane chat and for a full group with no empty pane; returns `null` when the open chat is a non-member (group backgrounded).
- **Existing gates green:** `pnpm exec vitest run`, `pnpm exec tsc --noEmit`, `pnpm run lint`, `cargo test --workspace`. (No Rust change, so `windows.rs` coverage is unchanged and already sufficient — the arbitration is not the defect.)
- **Manual two-window smoke** (for the human — do not launch the GUI from the agent):
  1. Window M: open chat X, split 2-col, fill pane 2 with Y. Open chat X (or Y) in a second window S via the sidebar's "Open in new window" so S shares a member; click back into M so M owns its panes.
  2. In M, archive one pane (the pane empties in place; the other keeps its chat; focus stays on the live pane).
  3. In M, hit New chat via the sidebar row **and** via ⌘T.
  4. **Before:** the split collapses, the new chat opens full-screen in M, the emptied pane is gone, and the member shared with S becomes driven by S (S's overlay clears).
  5. **After:** the new chat fills the emptied pane, the split stays intact, M shows no `DuplicateSubjectOverlay`, M still owns every pane, and S's state is untouched.

### Relevant code

- `src/components/Sidebar.tsx:1090-1098` — sidebar new-chat `StartChatModal.onStarted` (**the bug**: navigate-only).
- `src/components/Sidebar.tsx:406-410`, `:924`, `:983` — ⌘T + "new chat" row → the same `creatingChat` modal.
- `src/components/Sidebar.tsx:799-845` — `openDirectChat` pick path (fills the *focused* empty pane — the precedent to mirror).
- `src/pages/RunnerChat.tsx:1765-1783` — empty-pane button `StartChatModal.onStarted` (path a; already fills its target pane).
- `src/pages/RunnerChat.tsx:1217-1271` — `archiveSession` (leaves the emptied pane unfocused, URL on a live member).
- `src/pages/RunnerChat.tsx:293-320` — `subjectIds` / `useReportSubjects`: why backgrounding the group drops the members' subjects.
- `src/pages/RunnerChat.tsx:294`, `src/lib/paneLayout.ts:92-101` — `splitActive` / `isGroupActiveFor` (non-member ⇒ group backgrounds).
- `src/lib/paneLayout.ts:180-238`, `:482-494` — `applyPresetPure`, `assignSessionPure`/`removeSessionPure`, `assignSessionToPane`/`focusPane` (where `newChatTargetPane` lands).
- `src/lib/windowFocus.ts:100-166` — `useReportSubjects`, `isSecondaryFor` (arbitration; sound — not the defect).
- `src-tauri/src/windows.rs`, `src-tauri/src/commands/window.rs`, `src-tauri/src/commands/session.rs:510` — registry + spawn (verified: spawn never assigns window ownership).

### Follow-up: terminology correction — the "separate window" was a TAB

Continued dogfooding resolved a vocabulary mismatch that shaped this whole doc: in the user's reports, "window" meant **a new group of panes on the same chat surface** — what the hierarchy now calls a **tab** (arch §3.6, Window → Tab → Pane) — not an OS window. There was never a second OS window in the repro. Consequences:

- The multi-window ownership mechanism analyzed above is real machinery and the analysis stands, but it was not what the user was reporting. The observed bug in the user's vocabulary — "the new chat lands in a separate window" — reads correctly as: **the new chat opens as a new single-pane tab instead of filling the empty pane of the current tab.**
- An interim ⌘N → "File → New Chat" menu rebinding (built on the OS-window misreading) was reverted; ⌘N remains **New Window**, matching the hierarchy (⌘N = window, ⌘T = tab, panes fill from the pane's New chat button or a sidebar pick).
- The sidebar new-chat fill shipped above is correct and stays. The still-open defect is that the **empty-pane New chat button** (`RunnerChat`'s pane modal, path a) reportedly also lands the chat in a new tab, despite `assignSessionToPane` + `focusPane` + navigate reading correct. That path is instrumented (`[pane-fill]` console logs around the assign) pending a captured repro; the fix lands here once the logs identify the failing step.
- A later regression made the implementation treat the window as having only one persistent pane tab, so creating panes from a non-member chat replaced the previous pane tab. That was corrected by storing pane tabs as a per-window set keyed by member sessions: non-member pane creation now adds another tab, and reopening any member re-activates its existing tab.

### Follow-up: one-by-one resume scrollback corruption

After an app restart that restored a multi-pane **tab** with every **pane** stopped, resuming panes one by one could duplicate and reflow-garble Codex banner frames in scrollback while live prompt regions stayed clean. The same panes resumed cleanly through topbar **Resume all**, which made the failure specific to the per-pane resume sequence in an already-visible tab, not to backend replay or the resume fork dimensions.

The captured writer is `src/components/RunnerTerminal.tsx:737-779`, `refitAndPush()` calling `pushSize()`, whose `src/components/RunnerTerminal.tsx:692-710` path writes the local `ESC[2J ESC[H` clear for Codex/Claude runtimes and immediately sends `session_resize`. The proof lines were the enabled-pane refits to transient collapsed grids, each followed by `push-size`: `56x40 -> 56x19 disabled=false` then `push-size ... rows=19`, `46x18 -> 46x7 disabled=false` then `push-size ... rows=7`, and another `46x18 -> 46x7 disabled=false` then `push-size ... rows=7`. That path was not the activation wake dance (`dance=false` and no `resize-dance` lines during the settle path); it was a later resize observer/window resize refit hitting siblings that were already enabled in the one-by-one flow.

Root cause: `refitAndPush()` mutated xterm with `fit.fit()` before deciding whether the proposed grid was stable. During the resume sequence, a whole-viewport/pane-height transient proposed a much shorter row count for every visible pane. Because the panes were enabled, `pushSize()` treated that transient as real: it cleared the visible xterm region, pushed the collapsed PTY size, and Codex repainted at the tiny height. When the layout returned to the real size, the tiny-height repaint and clear had already been committed into scrollback, producing the duplicated/reflow-garbled banner frames. Resume all avoided the bug because sibling panes stayed transitional/disabled during the burst, so this enabled-pane writer did not run for them.

Fix: gate `refitAndPush()` before `fit.fit()` by using `FitAddon.proposeDimensions()`. For clear-on-resize TUI runtimes, a large row drop is delayed for a short stable retry; duplicate observer events stay delayed, and only the scheduled retry may apply the same pending collapsed dimensions. If the collapse was transient, the restored size wins without any local xterm reflow, local clear, or backend resize at the collapsed rows. Small ordinary resizes and shell-like runtimes keep the existing immediate resize behavior.

## #478 — split outcome

Source: [original record](https://github.com/yicheng47/runner/blob/8462b5194e0f1945ef62b1af1a2758c6816fd70d/docs/impls/archive/478-consolidation-pass-4.md).

Pass 2's three gates, method unchanged, **run once per surface**. Reuse the audit rather than writing a new one; it takes the surface as a parameter.

1. **AST item equality**, keyed by `Type::method`, validated by the five negative controls. Report two separate results, one per surface, and say so — a single combined pass number would hide a regression in one tree.
2. **Test count.** `cargo test --workspace 2>&1 | grep -c '^test .* ok$'` is **1179** on `main` at the branch point, measured after pass 3 landed. It must still be 1179.
3. **`make verify` green**, plus `cargo clippy -p runner-app --features updater --all-targets -- -D warnings`. One untouched run, cited honestly. Set `pipefail` or read cargo's status directly. Four `warning:` lines are expected and are on `main` too: the future-incompatibility notice for `block` and `proc-macro-error2`, once per phase.

Also report, per surface: the line count of every new file; every `pub(super)` item as `file::item` with a count; that `surfaces/mod.rs` is absent from the diff; the platform-cfg grep result; and confirmation that the two resolution structs moved and `CrewNameRefresh` did not.

Both cuts landed as prescribed. `crews/` is 10 files, largest `add_slot.rs` at 750; `runners/` is 10 files, largest `logic.rs` at 562. Seventy-six compiler-proven `pub(super)` items, `surfaces/mod.rs` untouched, no shared module created. The three predicted field-privacy items resolved as written: `RunnerEditResolution` and `RuntimeLayerResolution` moved to `runners/mod.rs`, `CrewNameRefresh` took `pub(super)`. One the brief missed, `RunnerFormKind`, needed `pub(super)` for the same reason and the compiler found it.

## #529 — native theme follow-up, 2026-09-11

Source: [original record](https://github.com/yicheng47/runner/blob/8462b5194e0f1945ef62b1af1a2758c6816fd70d/docs/impls/archive/529-runner-light-theme.md).

Four commits landed on the branch after the crew's PR opened: the Resuming pill moved to the accent family; Claude Code is spawned with `"theme": "auto"` in the `--settings` JSON Runner already sends, so it follows the terminal's background answer instead of the user's pinned theme; Rosé Pine Dawn joined the terminal list as a third-party light palette (canvas frame `VLI02`); Monokai left the list for its licence, a stored `monokai` loading as Match app. The lists below describe the code at launch.

## #553 — post-smoke placement decision, 2026-09-11

Source: [original record](https://github.com/yicheng47/runner/blob/8462b5194e0f1945ef62b1af1a2758c6816fd70d/docs/impls/archive/553-settings-scrollbar-alignment.md).

The column-edge placement below shipped on the branch and Jason rejected it on the Archived pane: the thumb sat on the card's right border and crowded the filter chips and the Delete all button. The reference he pointed at (the Codex app's Settings) keeps the bar at the window edge; the only real defects were the track starting at the very top behind the titlebar strip and running into the window's bottom corner. Final shape: the scrollbar stays at the window's right edge in an overlay inset by `TITLEBAR_DRAG_HEIGHT` at both top and bottom, with no column mirroring and no new constants. The Runners and Crews pages had the same overlap through `PaginatedListPage`, whose scrollbar sat inside the padded column on the cards' edge; it is pushed out by the page's side padding so the thumb rides the page edge, bounded by the list region (below the search row, above the pager). The sections below are the original plan and stay as the record.

## #590 — scope reduction and handoff, 2026-09-14

Source: [original record](https://github.com/yicheng47/runner/blob/8462b5194e0f1945ef62b1af1a2758c6816fd70d/docs/impls/archive/590-runtime-model-discovery.md).

Continue [PR #591](https://github.com/yicheng47/runner/pull/591) for [#590](https://github.com/yicheng47/runner/issues/590) on `fix/codex-model-discovery`. The starting implementation is `15e9834bf03c5c8a4a1d00230862b3477ca789fb`. The [feature spec](../../features/archive/590-runtime-model-discovery.md) defines the current scope.

Jason stopped the original Claude crew and requested inline completion, then explicitly requested removing the overbuilt parts on 2026-09-14. This replaces the original mission brief. Implementation stayed inline in the existing checkout. Jason confirmed the final macOS smoke test passed and authorized committing, updating PR #591, merging it, and preparing v0.9.1. Native Windows manual verification remains pending.

## #624 — implementation history, 2026-09-17

Source: [original record](https://github.com/yicheng47/runner/blob/8462b5194e0f1945ef62b1af1a2758c6816fd70d/docs/impls/archive/624-status-details.md).

Shipped in [PR #631](https://github.com/yicheng47/runner/pull/631) on 2026-09-17, both CI jobs green, after Jason's macOS smoke pass (an overall pass on the dev build; the Windows column of the [smoke record](../../tests/archive/624-status-details-smoke.md) is still open). Design signed off the same day: four frames on `design/runner.pen` (`z92Zy9`, `qKf92`, `HiZ5G`, `bqwlq`). Mission 1 ran on `feat/624-status-details` with the codex peer crew, all four items in one PR after Jason folded the working detail in; brief at [`gpui-rewrite/briefs/624-m1-status-details.md`](https://github.com/yicheng47/runner/blob/c1862492a9d3d7397de6571d13a10db824fe5d9b/docs/impls/briefs/624-m1-status-details.md).

- 2026-09-17 — Hook-capability audit of the five deferred items against the three adapters and the registered hook lists (table in the spec). Four design frames built as copies of the 347 spec rows and signed off. Spec, plan and mission-1 brief committed on `feat/624-status-details`; mission 1 started with the codex peer crew. Later the same day Jason folded the working detail (planned as mission 2) into mission 1; brief, plan and spec updated and the crew told through the channel. First review round: two must-fix (manual `/compact` from Idle stuck at Working; single-pane row tooltip losing `1 unread response`), three should-fix; decisions 2 and 5 refined. Jason tested the first dev build and overturned decision 1: the detail moves into the header and tab-bar label; frames, spec and crew updated.
- 2026-09-17 — Mission 1 implemented all four details in the shared macOS/Windows status path: presentation strings and lifecycle-local timers, transient failure attention and acknowledgement, hook-owned tool/compaction detail in all three adapters, and Claude Code compaction-hook registration. The two-crate CI-profile test command, workspace Clippy with warnings denied, formatting check, and diff check passed; the remaining live visual, CLI-signal, and refresh-cadence checks are recorded in [`docs/tests/archive/624-status-details-smoke.md`](../../tests/archive/624-status-details-smoke.md).
- 2026-09-17 — Third review round clean (tooltip identity via keyed state, the two `/compact` round-trip holes). Own pass and the four checks green, PR #631 opened, both CI jobs green. Jason tested the dev build: the header label is right and the duplicate hover tooltip goes (commit `9a13ee9`); a Codex steer test showed plain Idle because Codex fires `Interrupt` without `UserPromptSubmit` for a steered prompt, a 347 gap noted for a follow-up. Overall macOS smoke pass reported; merged. Codex shutdown after Ctrl+D measured at about four seconds outside Runner, Codex's own exit work, not a Runner issue.

## #632 — verification and implementation log, 2026-09-17

Source: [original record](https://github.com/yicheng47/runner/blob/8462b5194e0f1945ef62b1af1a2758c6816fd70d/docs/impls/archive/632-session-status-signal.md).

`cargo test --workspace` (814 backend tests, every app suite, the CLI roundtrip asserting `session_status`), workspace Clippy with `-D warnings`, `cargo fmt --check`, `git diff --check`. Legacy-row tests: `reconstruct_and_dispatch_read_legacy_runner_status_rows` (router), `event_projection_reads_legacy_runner_status_rows` (MCP `mission_status`), the pre-#632 row at the end of the mission activity test (`ops/mission.rs`), `session_status_projection_reads_legacy_runner_status_rows` (app), and the `from_name` / `is_session_status` cases in runner-core.

- 2026-09-17 — Implemented inline after #624 landed: a mechanical rename pass over the workspace, then the read-compatibility helpers in runner-core, the four reader sites, the CLI canonicalization, the prompt wording, the docs and the mission-watch filter. Design canvas labels, the third 604 follow-up, stay open.

## #977 — post-0.17 cleanup outcomes

Source: the deleted plans for the [workspace client](https://github.com/yicheng47/runner/blob/a23d578f1b6a07312df580f658de3a46f3f14248/docs/impls/977-workspace-client.md), [remote client owner](https://github.com/yicheng47/runner/blob/a23d578f1b6a07312df580f658de3a46f3f14248/docs/impls/977-remote-root.md), [app layout](https://github.com/yicheng47/runner/blob/a23d578f1b6a07312df580f658de3a46f3f14248/docs/impls/977-app-layout.md) and [CLI and daemon splits](https://github.com/yicheng47/runner/blob/a23d578f1b6a07312df580f658de3a46f3f14248/docs/impls/977-cli-daemon-splits.md), extracted in the 0.19.0 sweep. Validation records: [app layout](../../tests/archive/977-app-layout.md), [remote client owner](../../tests/archive/977-remote-root.md) and [workspace client](../../tests/archive/977-workspace-client.md). The six cleanup PRs merged on 2026-10-28 (shared helpers #1054, runtime adapters #1056, workspace client #1057, remote client owner #1059, CLI/daemon splits #1060, app layout #1062), and #977 closed on 2026-10-29. Each was a behavior-preserving refactor with no live checks.

- **Workspace client (#1057).** `FrontPageClient` became the workspace operation interface for both targets, with local operations using the same `DaemonClient` calls as before; presentation, attachment and refresh handlers stayed on their GPUI entities, and remote ownership was left to #1059. Production scan from `bbc3a074`: `NativeRoot` methods 577 → 577, `remote_window` references 279 → 263, mission-workspace `self.remote` references 88 → 75.
- **Remote client owner (#1059).** `RemoteClient` is a plain struct held by `NativeRoot`, not a second GPUI entity: an entity would force nested entity updates while the root renders shared panes and handles actions, and keeping `Context<NativeRoot>` preserves synchronous notification, focus and completion ordering. Production scan from `e0b69671`: `NativeRoot` methods 577 → 544, `remote_window` references 263 → 0, mission-workspace `self.remote` references 75 → 14; the record has the method.
- **CLI and daemon splits (#1060).** `runner-cli/src/command.rs` split into the clap tree, per-noun handlers, feed, status and shared helpers; `session/manager/spawn.rs` split into spawn spec, mission, direct, fork, resume, capture and shell modules. The staged CLI suite and both daemon stages passed (1,504 daemon tests each); English and Chinese `make guide-cli` output was byte-identical; 135 CLI and 124 manager functions plus 49 types were preserved. Two full `make verify` runs at the inherited 10,240 file-descriptor limit failed to open a temporary database in the unchanged app test `surfaces::start_chat::tests::confirm_binding_precedes_every_control_and_preserves_open_menu_values`, which passed alone; the gate passed with `ulimit -n 65536 && RUST_TEST_THREADS=8 make verify`.
- **App layout (#1062).** The library became the app's single module tree behind a thin `main.rs`, and large production files were split along existing responsibility boundaries; the record has the parser audit and results.
