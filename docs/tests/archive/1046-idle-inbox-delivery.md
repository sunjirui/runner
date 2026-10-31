# #1046 — Idle inbox delivery and draft cap: macOS QA

Date: 2026-10-30, 07:33–07:46 UTC. QA followed the mission's selected matrix after the reviewer's clean working-tree review and coder's handoff. **NO REMAINING MUST-FIX QA ISSUES within the selected macOS matrix.** Windows MIS-MSG-03 was not run and remains with Jason. This is feature validation, not a full regression pass.

## Candidate and environment

Candidate: `8818be0f66e9765f3fb1774a9d435ab46cd0006c`, branch `feat/1046-idle-inbox-delivery`, worktree `/Volumes/samsung/repos/yicheng47/runner/.worktrees/feat-1046-idle-inbox-delivery` (the mission's `/Users/jason/repos/…` path resolves to it). Initial status was clean. The implementation matches reviewer snapshot SHA-256 `6ab5ac8b605936da5648325442b6e5ec5cdcc3b54fb04857ce6055f6e6c1cd73`. Only the explicitly authorized temporary cap substitution changed implementation source during QA; it was restored before the test record was written.

Host: macOS 26.6.2, build 25G83. Runner app and development CLI: 0.18.3. Codex CLI: 0.165.0, model `gpt-6-luna`, effort `low`. Claude Code: 2.1.314, model alias `haiku`, displayed as Haiku 5.5; it inherited `medium` effort, a test-execution deviation from the requested low effort. No global effort setting was changed. Missions used Runner's fixed Bypass launch policy. Claude's test-folder trust was accepted under standing authorization.

Build: `env -u NO_COLOR make run`, using this worktree's `target/`. Native control used `cua_repl` bound specifically to `com.wycstudios.runner.qa1046`, a temporary wrapper at `/private/tmp/runner-1046-qa-a1sezpsn/Runner 1046 QA.app`. Its executable and CLI sidecar were copied from this worktree and compared by SHA-256. The app executable hash was `35fa634a89b6b0a493d34fd73df0cbccc525ede2383169b36a50c07f0a1dc54a`; initial CLI sidecar hash was `c032dc4124eaf7cd7c435371d52c0d823961157e99341d8d431a14862b00d143`. The endpoint was independently verified as `/Users/jason/Library/Application Support/com.wycstudios.runner-dev/runnerd.sock`. New chat was opened and dismissed as a harmless native-control check before terminal input. Installed Runner was not operated.

External development CLI commands used the absolute development sidecar and removed `RUNNER_CREW_ID`, `RUNNER_MISSION_ID`, `RUNNER_HANDLE` and `RUNNER_EVENT_LOG`. The host has no supported external feed watcher that pushes output across turns; that limitation was disclosed. Finite `mission show`, `session show` and `mission feed --all --oldest-first --limit 500 --json` captures supplied independent state/evidence. No automatic-watch claim is made.

Evidence root: `/private/tmp/runner-1046-qa-a1sezpsn/evidence/`. `delivery-summary.json` maps every test message to its covering read and exact ACK-file count. `codex-final-feed.json`, `claude-final-feed.json`, the per-runtime ACK files, native PNGs and build logs remain there. The initial build log is `/tmp/runner-1046-qa-build.log`. Raw local configuration snapshots are private scratch evidence and are not included in this repository.

## Changed behaviors and selected checks

| Changed behavior | Selected check and reason | Result |
| --- | --- | --- |
| Working recipients queue notices until Idle. | MIS-MSG-01 Working→Idle variant on Claude and Codex; inspect native composer during the bounded first turn, then require a submitted notice, covering read and one ACK marker. | Passed |
| Startup/first-turn messages arrive once after the turn. | MIS-MSG-06 selected long-first-turn variant on Claude and Codex, with one 35-second terminal task. | Passed |
| A recent observed draft preserves the person's text. | MIS-DRAFT-01 Claude and Codex: type an unsubmitted marker, post a message, observe Inbox waiting beyond the two-second guard, then clear without Return. | Passed |
| The observed-draft hold expires after its cap. | MIS-DRAFT-01 Claude and Codex with a 30-second development-only cap; leave the draft untouched and verify automatic submission of draft plus notice. | Passed |
| An app connection cannot leave composition holding delivery. | MIS-DRAFT-08: start native Chinese Pinyin preedit in idle Claude, terminate only the QA app without cancelling/committing it, then post through the development CLI while the app remains closed. | Passed, using abrupt app termination as described below |
| Windows Codex delivery during tool windows. | MIS-MSG-03 requires native Windows. No Windows host/native-control surface was available in this session. | Blocked; not run, left to Jason |

The coder/reviewer reported the selected automated tests and workspace gates green before handoff; QA did not rerun them. Pi's uncapped native draft, cursor/revision ownership, internal InputCleared publication, terminal-control handover and failed-clear retry are covered by that automated evidence, not claimed as extra live checks here. No unrelated runtime, resume, permission or full-smoke matrix was added. The first-turn test uses human-addressed messages in two disposable single-slot missions; peer-sender, add-role and output-only runtime variants were outside the brief's selected live variant.

## Native results and event evidence

Each runtime ran the bounded goal once, writing `started.txt`, waiting 35 seconds, writing `finished.txt`, then replying `QA1046-FIRST-DONE`. Both marker files appeared in that runtime's canonical test directory. Messages posted during Working were absent from the native composer during the turn. Codex first reached completed Idle at `07:36:56.412Z`, then read the message at `07:36:58.728Z`; Claude completed at `07:38:12.640Z`, then read at `07:38:16.038Z`. Native terminal history showed the first-turn final response before the submitted inbox notice and ACK. Each first-turn marker occurs exactly once in its ACK file.

For draft preservation, `QA1046-DRAFT-CODEX-preserve` and `QA1046-DRAFT-CLAUDE-preserve` remained intact with **Inbox waiting** visible and no notice inserted. Clearing all text with End followed by Ctrl-U, without Return or Submit / clear, released one clean notice. The first Codex clear removed only the text before the cursor; the remaining suffix continued to hold correctly until it too was cleared. No draft text was submitted in the clear variant.

| Variant | Message event | Covering inbox_read event | ACK-file count |
| --- | --- | --- | --- |
| Codex Working/first turn | `01M65VT53DZWG3RRHKQZX2J00R` | `01M65VV438GYC8SRKDQ3CDR3H9` | 1 |
| Claude Working/first turn | `01M65VWJHDYA58AE91TMMAGNV0` | `01M65VXFK6BE7BDG2MZERMHQPZ` | 1 |
| Codex draft clear | `01M65VX93CR0EXWMJXS04GV402` | `01M65VY2HN297PF64PGN84PCC8` | 1 |
| Claude draft clear | `01M65VYP2KC0JJNM89XHHVDC9A` | `01M65VZ53BM1Y689DSDX41PQBQ` | 1 |
| Codex 30-second cap | `01M65W4YZZ654N7JRVDKNMF020` | `01M65W5T6G9XB2QD7NRJ1Q9G1Y` | 1 |
| Claude 30-second cap | `01M65W46THJTPSNY26RA4AVFED` | `01M65W5SQ2M648HFZ779PQD04A` | 1 |
| Claude app disconnect | `01M65W29RTW1FAN7MJP0392K33` | `01M65W2DM7CW3M5H4T6VQ2NTQE` | 1 |
| Claude final disconnect with saved native PNGs | `01M65W8SA8835X9JRV8VYF8731` | `01M65W8Y2DS9R4DPSK43E7FD7K` | 1 |

For the cap build, `OBSERVED_DRAFT_HOLD_CAP` alone changed from `Duration::from_secs(10 * 60)` to `Duration::from_secs(30)`. `cargo build -p runner-cli` rebuilt the daemon-bearing sidecar; `cargo build -p runner-app` confirmed the native app build. The shortened sidecar hash was `e13f113b1f3b16bd460a5382b8cf1c17168f2f9bc0a6c76f6cc52fc682e6da70`. The old QA app and development daemon were stopped, the shortened sidecar was copied into the wrapper, and the wrapper relaunched. The source file was immediately restored byte-for-byte from HEAD; `git status --short` was empty. `cap-only.diff`, `cap-build.log` and `cap-app-build.log` preserve that build's identity.

Both cap drafts initially held. With no further typing, the eligible attempt appended the notice and submitted it together with `QA1046-DRAFT-…-cap`. The native history visibly showed the combined prompt and eventual ACK; `codex-cap-held.png`, `codex-cap-after.png` and `claude-cap-after.png` retain it. The first notice can follow the cap on the router's reconciliation cycle; the cap is not a promise of submission at exactly 30 seconds. Claude's covering read was at `07:42:48.547Z`, Codex's at `07:42:49.040Z`.

For orphaned composition, the enabled Simplified Chinese Pinyin input method produced underlined `ni hao` preedit in idle Claude without writing terminal text. Cmd-Q was consumed by the input method; choosing Quit from the app menu cancelled the preedit before opening the quit confirmation. Consequently that exploratory attempt does not prove connection-close cleanup, and its ORPHAN/AFTERQUIT messages are not counted as the disconnect pass. The dialog was cancelled, preedit recreated, and only the exact ledger-owned wrapper PID terminated with SIGTERM. The daemon and agent session stayed running. A new message posted after termination was read in four seconds without reopening the app, pressing Return, or using Submit / clear.

After rebuilding and relaunching the restored ten-minute candidate, the same abrupt-close variant was repeated to save durable native before/after screenshots: `claude-orphan-preedit.png`, `final-disconnect.txt`, `claude-finaldisconnect-message.json`, and `claude-orphan-recovered.png`. The final message was posted at `07:44:26.440Z`, read at `07:44:31.309Z`, and ACKed once. The recovered native composer was empty. `claude-user-prompts.json`, extracted only from this test conversation, contains neither `ni hao` nor `你好`; the composition never reached a submitted prompt. This establishes abrupt app-connection loss behavior. Normal menu-driven quit cancels composition earlier; internal InputCleared and control-handover behavior were not directly instrumented in this live run.

## Cleanup and remaining limits

Owned fixtures: roles `qa1046-codex` (`01M65VSBBYB91WJNZYAYSVP4AT`) and `qa1046-claude` (`01M65VSBBDNWCATB4MC48CF49E`); crews `01M65VSBC6A4G91D7PEF40Z7JF` and `01M65VSBBN8TTQFC0722D2MW0W`; missions `01M65VSJ2MKRYZV6SN9BYTDAK4` and `01M65VTS6ZMTHAK79Z9HJJKZ6W`; sessions `01M65VSJ2P1HQDKH8FSV4RVDTP` and `01M65VTS71TZ0KSRRJG5HQV5NX`. Canonical directories were `/private/tmp/runner-1046-qa-a1sezpsn/codex` and `/private/tmp/runner-1046-qa-a1sezpsn/claude`. Full creation data is in `ledger.json` at the scratch root. Roles, crews and archived conversation history are retained to preserve evidence.

Both missions were stopped and archived through the development CLI. Readback reports zero live sessions and archive timestamps `07:45:21.702Z` and `07:45:22.225Z`; `codex-cleanup.json` and `claude-cleanup.json` preserve it. Native search no longer listed either mission (`cleanup-search.png`). Only the QA wrapper was terminated and its LaunchServices registration removed. The wrapper and evidence files remain in scratch. Pre-existing development sessions were not typed into, archived or otherwise individually operated; installed Runner was untouched.

The normal ten-minute sidecar was rebuilt and relaunched before final validation/cleanup. Its final hash was `082e6454499e61e5a3a984182195fd6b11a3651f0bb17410fd42b8761325441c`; `restored-build.log` records the build. No shortened cap is left in source or installed development binaries. Git status was clean after restoration and immediately before creating this record; `git diff --check` passed. This Markdown record is left uncommitted for the coder.

Only this run's Codex trust entry was removed; the complete config returned byte-for-byte to its pre-run snapshot (`codex-config-final.diff` is empty). Claude used the existing `.claude-work` account directory, discovered from its test transcript rather than the default `.claude.json` snapshot taken before launch. Its actual account's pre-launch projects snapshot is therefore unavailable. Cleanup removed only the exact new canonical test-folder project entry from `.claude-work/.claude.json` and verified the rest of its projects map unchanged against the pre-cleanup snapshot; `claude-projects-cleanup.diff` records this narrow cleanup. No authentication, effort, extension or permission settings were changed to obtain a pass.

No confirmed product failures remain from these checks. Windows MIS-MSG-03 is the required disclosed platform gap and remains for Jason. Live control handover was not exercised because no second terminal-control client was used. The unchanged two-minute warning and pi-native cap exception were not added to this narrow live matrix. All selected accessible macOS workflows passed with the method and configuration limits above.

## Regression maintenance, 0.19.0

Its selected checks are MIS-MSG-01, MIS-MSG-03, MIS-MSG-06, MIS-DRAFT-01, MIS-DRAFT-02 and MIS-DRAFT-08 in [missions](../regression/missions.md), updated in the implementing PR. The Windows MIS-MSG-03 gap and the temporary 30-second cap method remain evidence above.
