# #977 remote client owner — validation

Scope: `docs/impls/977-remote-root.md`, branch `refactor/977-remote-root`, based on `e0b69671`. This changes ownership and callback routing without intended behavior changes. Neither duo slot launches the app; live checks belong to Jason.

## Selected checks, recorded before execution

| Moved behavior | Check and reason |
| --- | --- |
| Computer table, picker, pairing, target switches and remembered selection | Existing `remote_client::tests` covers identity changes, PIN states, pairing generations, returning local and restoring each computer's sidebar/tab/focus. |
| Overview reconciliation, connection state, terminal attachment/control/sizing/settings | Existing remote-client tests cover delayed attachments, transport/identity epochs, untrusted/revoked connections, pane visibility, terminal lifetime, resize, control failures and host/viewer palettes. |
| Remote libraries and shared-action completions | Existing remote-client/front-page tests cover catalog request/target guards, pending controls, stale completions, host topology, navigation, shell placement retry and capability gates. |
| Mission presentation snapshot/member/status/settings/feed and selected member | Existing `mission_workspace::remote_tests` covers remote member projection, shared feed history/lifetime, switching/reconnection, shell retry, lifecycle/composer action generations and focus/dialog resets. Existing mission tests cover native presentation. |
| Host enrollment/settings callers now reach the viewer owner | Existing settings-remote tests remain in the complete app run; host enrollment logic stays in place. |
| Shared native callers and all platform compilation | Complete `cargo test -p runner-app`, then `make clippy` (including updater). `make fmt` and `make verify` cover formatting and the required workspace gate. |

The app/workspace test commands use a command-local `ulimit -n 32768`, matching the pooled SQLite descriptor-pressure finding recorded in #1057. No test filters or additional skips are used and no global configuration changes. Existing test changes only qualify moved methods/state through `remote_client`, `workspace()`/`workspace_mut()` and the mission `presentation` owner; fixtures and assertions retain their meaning. There is no new behavior at the boundary requiring an additional test.

Moved-symbol edits occur in `remote_client/tests.rs`, `front_page/tests.rs`, `mission_workspace/{remote_tests,tests}.rs`, `crews/tests.rs`, `roles/tests.rs`, `sidebar/tests.rs`, and inline tests in `command_palette.rs`, `front_page/{actions,folder_picker}.rs` and `start_mission.rs`. Host-settings test fixtures/assertions are unchanged.

## Results

The first reviewed working-tree candidate passed complete `cargo test -p runner-app` (771 binary tests, 168 library tests and integration suites), `make clippy` including updater, `make fmt`, `make verify` (workspace check/tests/Clippy/format) and `git diff --check`. The existing manual/real-provider ignored tests retain their default status. The final workspace gate includes the remote chat catalog and project-folder picker moves, with the same app test counts. No selected automated check failed.

## Working-tree review and live handoff

Working-tree re-review on 2026-10-28 is **clean, no remaining must-fix issues**. The reviewer checked the complete initial diff, every revision, moved methods/callers, guard/lifetime/retry coverage and passing logs, and independently reproduced the counts. The reviewed patch SHA-256 is `110c08c142fb1f00400b48767fb7326a9d4896ae0d2ad6d77eb4484b384d919b`. The final recorded verdict and live selection are the only subsequent changes before committing.

All checks below are **NOT RUN**, left to Jason in both client/host directions on two computers, together with #1057. This is selected feature coverage, not a full regression pass. Neither duo slot launched the app.

1. `REMOTE-PAIR-01`, `REMOTE-PAIR-03` and keyboard/dismissal portions of `REMOTE-PICK-01`: pair from Settings/picker, refusal/success, identity changes, Refresh and picker traversal/dismissal. These cover moved pairing/picker callbacks and focus. Use owned test pairings for identity/revoke/forget cases.
2. `REMOTE-WIN-01` and sidebar/selection portions of `REMOTE-WIN-02`: open in the same/new window, return local, revisit and close; retain each computer's tabs/split focus/sidebar settings and resume local terminals/feed. Include returning through a local notification with a remote window open. These cover the root bridge, remembered selection and drop paths.
3. `REMOTE-TERM-02` through `REMOTE-TERM-06` and revoke/forget portions of `REMOTE-PAIR-07`: resize chat/member/drawer, hand back/take control, disconnect/reconnect while typing, recover input/output/error/control state, and pair then Open immediately. These cover attachment lifetime, epoch, sizing, control and connection synchronization. A short typed/pasted marker suffices; no extra IME/runtime matrix.
4. `REMOTE-CHAT-01`, `REMOTE-CHAT-02`, `REMOTE-CHAT-04`, `REMOTE-WS-01` and `REMOTE-WS-02` action paths: host start/folder choices, lifecycle/rename, split/fill/resize/close, drawers, sidebar and disposable project deletion. Include #1057's empty-pane sibling-cwd and delayed pending-action cases, plus navigation/computer switching during outstanding actions. These cover launch tokens, layout hints, pending state and completion guards.
5. `REMOTE-WS-03`: host palette/sidebar results; role/crew search, paging, Refresh, empty/error recovery and row launch; return local and retain list preferences. These cover catalog guards and list projection.
6. `REMOTE-MIS-01` through `REMOTE-MIS-03`: start/failure form retention, member order/selection, drawer start/close/placement retry, feed pagination/post/answer/reconnect, lifecycle and navigation during pending completion. Open the same mission in two remote windows, leave each and revisit. These cover presentation caches, shared-feed release/catch-up, shell state and generations.

The reviewer requires no additional standalone local live cases for this PR: local dispatch is unchanged, and app tests cover moved local selectors/shared mission menu. #1057's previously named local live checks remain NOT RUN for the combined handoff. Host enrollment/authentication, daemon/protocol and full runtime/IME regressions remain outside this PR's added scope.


## Ownership review revision

The first working-tree review requested changes to ownership, while finding no runtime regression. R1 identified remote library paging, layout/resize state and launch completion bookkeeping still accessed through full-workspace borrows. R2 identified mission presentation caches, feed subscriptions, action state and shell retries still coordinated on `MissionWorkspace`. The revision moves those algorithms to the owning types, makes full viewer workspace inspection test-only, and makes mission `remote()`/`remote_mut()` selectors private. Existing fixture accesses now name the test-only inspection helpers; assertions and test data are unchanged.

Before revision gates: use the complete app run to cover role/crew filtering/paging/refresh, topology and resize drafts, serialized layout/new-chat pending state, project deletion, target guards and stale completions for R1; use the mission-remote cache/roster/feed/reconnect/dialog/action/shell-retry cases for R2. Run `make clippy` including updater, `make fmt` and `make verify` after the revised app run. The normal and test target compile check passes. The corrected revised candidate passed complete app tests (771 binary tests, 168 library tests and integration suites), `make clippy` including updater, `make fmt`, `make verify` and `git diff --check`. Working-tree re-review is clean, as recorded above.

The required literal counts are 577 → 544 root methods, 263 → 0 `remote_window` identifiers and 75 → 14 mission-source `self.remote` references. The comparable whitespace-tolerant mission consumer-access metric is 82 → 39, counting the old field versus the new mode/broad-selector calls inside `impl MissionWorkspace` only. All 39 remaining consumer calls are mode guards; none uses a broad remote-state accessor. Production viewer consumers have zero `workspace()`/`workspace_mut()` calls; those full-state APIs exist only for test inspection. The plan records the exact scan scope and deliberately retained boundaries.

The revised app suite passed (771 binary tests, 168 library tests and integration suites). The first revised Clippy gate rejected one unnecessary lazy fallback and the repeated shell-stage type complexity; the fallback is simplified and the existing stage type now has one shared alias. No tests failed; the corrected Clippy/workspace gates pass without suppressions.

## Regression maintenance, 0.19.0

None apply: this changed ownership and callback routing without intended behavior changes. Its automated checks stay with the owning tests, and no live case was added.
