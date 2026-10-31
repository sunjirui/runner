# #977 workspace client — validation

Scope: PR (a) from `docs/impls/977-workspace-client.md`, on `refactor/977-workspace-client`. This is an operation-dispatch refactor with no intended behavior change. No native app or live agent sessions were launched by this duo.

## Automated coverage

| Rerouted behavior | Selected check and reason |
| --- | --- |
| Local role/runtime chat and shell creation; role-list shortcut | `local_workspace_mutations_keep_daemon_requests_arguments_and_errors` checks the original daemon operation, scope, cwd, overrides and dimensions; existing start-chat and role tests check UI completion/default handling. |
| Stop, resume, restart, fork, rename, pin, archive and close | The request-parity test checks arguments and unchanged daemon errors; existing chat/sidebar tests cover placement, transitions, archive order and title handling. |
| Mission start, pin, rename, stop, resume and archive | Request parity plus existing mission-workspace/start-mission tests. `mission_lifecycle_entry_points_dispatch_once_for_both_targets` invokes stop/resume/archive/rename/slot-stop for both targets and checks dispatch/errors. `local_mission_resume_preserves_concurrent_refresh_partial_failure_and_dimensions` checks refresh ordering, continuing after failure, skipping running members and keeping the first error. |
| Sidebar move/reorder, tab and mission layout, project create/delete | Request parity checks persisted IDs, returned local row handling and confirmed-vs-checked deletion; existing sidebar/layout tests cover the shared reorder planner and completion behavior. |
| Mission messages and answers | Request parity checks original post/signal payloads; existing composer/feed/remote tests check drafts, recipient handling, questions, generations and feature gates. |
| Existing remote catalog/feed/folder dispatch | Existing front-page and remote-client suites; these methods retain their remote-only contract and Local remote-target errors. Native AppStore reads, feed handling and dialogs are unchanged. |
| Remote shell cwd when the focused pane is empty | `new_terminal_entry_points_inherit_remote_sibling_cwd_when_focused_pane_is_empty` invokes both New terminal entry points and checks that the sibling's custom folder wins over the project's default folder. |
| Remote pending controls and stale errors before a slow request completes | `remote_actions_publish_pending_and_clear_errors_before_held_requests_complete` observes root notifications during held stop/archive requests and checks pending state, cleared error and disabled stop controls before completion. |

Passed locally: `cargo test -p runner-app`, `make clippy` (including updater), `make fmt` and `make verify` (workspace check/tests/Clippy/format). The revised app-only runs cover the shared command paths, including the paired local/remote entry-point tests. CI is checked after the clean working-tree review and PR creation. An initial Clippy run found expression/slice cleanup issues, which were corrected before the full gate.

Only moved-symbol edits occur in existing remote tests: `MissionWorkspace.client` replaces `remote.client`, and `post_mission_composer` replaces `remote_post`. Assertions and fixtures stay unchanged.

## Live handoff — not run

The reviewer selected the following checks in the clean working-tree review on 2026-10-28. All are **NOT RUN**, left to Jason; neither duo slot launched the app. This is not a full regression pass.

1. Local chat: start runtime and role chats with explicit cwd/model/effort, including the role-list shortcut; stop, resume, fork, rename and archive an owned chat. Check placement/focus, conversation continuity and error feedback because native completion handlers consume shared client results.
2. Local workspace: split/fill/close panes and close a tab; add/switch/close chat and mission drawer shells and revisit the route (`WS-DRAWER-01` action portions). Check cwd, session identity and persisted layout because shell/close/layout dispatch changed.
3. Local sidebar: pin/unpin, rename, move between projects and reorder owned rows (`WS-SIDEBAR-01` action portions); create/delete a disposable project and verify running-member confirmation. These cover returned local node rows, shared rename/archive and checked-versus-confirmed deletion.
4. Local missions: start with a crew and with No crew; pin/rename; stop/resume/restart a slot and stop/resume/archive the mission (`MIS-LIFE-01` action portions); archive the viewed versus another owned mission from sidebar/workspace (`MIS-LIFE-03` relevant routes). Post to crew/selected handles and answer a native question (composer/card portions of `MIS-MSG-01` and `MIS-ASK-01`). Check destinations, no duplicate slots, recipient/answer payloads and retained fields/drafts on failure because mission lifecycle, start and communication handlers share the client.
5. Two-computer remote checks: `REMOTE-CHAT-01`; lifecycle/pending-action portions of `REMOTE-CHAT-04`; `REMOTE-WS-01` and `REMOTE-WS-02` action paths; `REMOTE-MIS-01`; post/answer/lifecycle/failure-draft portions of `REMOTE-MIS-02`. Include filling a focused empty pane through both terminal entry points with a sibling custom cwd, and delayed stop/archive to verify immediate pending controls and cleared stale errors. These cover retained host capability/generation guards and remote completion behavior.

Pairing, transport reconnection, long-history loading, IME and runtime adapter regressions are outside this selected scope. Global agent configuration and authentication are unchanged.


## Working-tree review iteration

The reviewer rejected the first candidate's remaining command-selection forks. The revised candidate consolidates the operation paths listed below. Both new paired entry-point tests pass, including chat archive confirmation. The mission-entry test initially used a nested root update while the action updated sidebar state; using the window test context corrected that fixture. Existing remote tests and assertions remain unchanged apart from the previously listed moved symbols. The revised candidate passed `cargo test -p runner-app`, `make clippy` (including updater), `make fmt` and `make verify`; the final app-only run also covers the archive-confirmation extension. The review verdict is reported through Runner and in the PR.

Re-review accepted the shared dispatch structure and found two behavior regressions: filling an empty remote shell pane lost the sibling-session cwd fallback, and direct action preparation did not publish pending/error changes. Both are corrected, with the two focused regression tests listed above. Their focused run passes; the cwd fixture's first runs omitted required overview fields (`unread`, then `missions`), which were corrected before the passing run. The corrected candidate passed `cargo test -p runner-app` (772 binary tests, 168 library tests and integration suites), `make clippy` (including updater), `make fmt`, `make verify` and `git diff --check` before final review.

The corrected candidate's first full app run passed 771 tests, including both new regressions, but `confirm_binding_precedes_every_control_and_preserves_open_menu_values` failed while opening a fresh temporary SQLite pool (`unable to open database file`). That unchanged test passed immediately in isolation without a code change. The failure repeated during `make verify` and a diagnostic app run. `lsof` showed retained temporary SQLite handles reaching descriptor 10,239, matching this shell's 10,240-descriptor limit. Subsequent complete app/workspace gates use a command-local `ulimit -n 32768`; no tests are skipped and no global configuration is changed. CI uses nextest's separate test processes.

Final working-tree verdict on 2026-10-28: **clean, no remaining must-fix issues**. The reviewer accepted the shared command paths, local request parity, caller tests, both behavior fixes and the recorded validation results. Permanent remote ownership extraction remains PR (b), awaiting Jason.

The operator's subsequent PR review identified unused local catalog, folder and feed implementations, with no native call sites. Those helpers and their dedicated test are removed, restoring the existing Local remote-target errors for all three methods. The active local mission-resume loop stays. Selected correction checks passed: complete `runner-app` tests (771 binary tests, 168 library tests and integration suites, covering remaining client/caller/remote paths), `make clippy` including updater (removed-helper/import validation), `make fmt` and `git diff --check`. The app command again uses the recorded command-local descriptor limit. Full `make verify` passed before this unused-reader removal; these requested checks cover the correction before quick working-tree re-review and amending/pushing.

Quick working-tree re-review on 2026-10-28 is **clean, no remaining must-fix issues**. The reviewer confirmed the original Local error before any daemon request, unchanged remote dispatch and retained mission-resume behavior. No additional live checks are needed for removing these unreachable readers; the selections above remain not run.

## Operation-path audit after working-tree review

The first candidate only wrapped local calls inside existing operation forks and was rejected at review. This candidate moves request execution into the following shared paths. Target checks around them retain validation, synchronous versus background scheduling, attachment and completion behavior.

| Entry points | Shared command path | Target-specific work retained |
| --- | --- | --- |
| Chat start and retry | `FrontPageClient::launch_chat` | Form defaults/size; remote staged retry; local title-save warning and native placement. |
| Chat stop/fork/resume | One client call in each handler, between `WorkspaceAction::prepare` and `complete` | Local transitions/focus; remote capability/generation guards and snapshot reconciliation. |
| Shell creation | `WorkspaceAction::shell`; mission drawer's shared operation closure | Local cwd and native layout completion; remote staged session/layout retry. |
| Pane move/split/close and layout saves | `save_workspace_layout` | Local tab cache and attachment; remote layout hints and host pushes. |
| Chat/pane/sidebar rename | `submit_workspace_rename` / `submit_sidebar_rename` operation closures | Local synchronous inline rename, remote async guards, unchanged/empty-title rules and field errors. |
| Mission slot and lifecycle actions | One call to the same typed client method in each handler | Local returned mission/session rows and partial-resume errors; remote reopen IDs and pending action state. |
| Archive and terminal-tab close | `WorkspaceAction::archive` and `FrontPageClient::archive_sessions` | Local sidebar pending markers and partial errors; remote layout reconciliation. Confirmation dispatch is shared. |
| Sidebar pin/move/reorder/mission archive | One operation closure per command | Native returned node rows or remote host push completion. |
| Project deletion | `confirm_delete_project` operation closure | Native confirmed deletion versus remote running-member confirmation, with existing error text. |

Remaining target forks choose the computer/pairing UI, palettes/chrome, native versus host catalogs and folder pickers, local-only configuration editors, layout/cache ownership, terminal transport, or completion scheduling. They do not select a second command implementation for the audited operations. The method/reference counts in the plan measure this retained presentation ownership separately; PR (b) still requires Jason's next authorization.

## Regression maintenance, 0.19.0

None apply: this rerouted operation dispatch without intended behavior changes. Its automated checks stay with the owning tests, and no live case was added.
