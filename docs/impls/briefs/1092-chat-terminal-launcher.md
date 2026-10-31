# #1092 — One launcher for starting chats and terminals

Jason requested a `codex trio` mission for [#1092](https://github.com/yicheng47/runner/issues/1092) on 2026-10-31. Work only in `/Users/jason/repos/yicheng47/runner/.worktrees/feat-1092-chat-terminal-launcher` on `feat/1092-chat-terminal-launcher`, based on `origin/main` at `07e1a4ba767531447a8d864776557194a48a8533`. The root checkout stays on `main`. Use this worktree's own `target/`; do not set a shared `CARGO_TARGET_DIR`.

## Problem and outcome

Starting a chat takes two steps per pane: an empty pane asks “chat or terminal?”, then New chat opens the full Start a chat form. One launcher, drawn inside the pane, replaces the empty-pane picker, the Start something new default page and the Start a chat form: a search field, the start folder, the four most recently used agents and roles, and pinned Terminal (and Mission on the default page) rows. A click, 1–4 or ↵ starts a chat with no form.

The spec is [`docs/features/1092-chat-terminal-launcher.md`](../../features/1092-chat-terminal-launcher.md); read it in full first. Where this brief and the spec disagree, the spec wins; where the spec and the PNGs disagree on detail, the spec wins, and anything neither settles goes to the orchestrator through Runner.

The design is exported in this worktree's ignored `target/design-ref/`: `launcher-in-pane.png`, `launcher-default-page.png`, `launcher-typing.png` and `launcher-click-table.png`. Match their layout and spacing with existing theme tokens; terminal text and agent-row model labels are illustrative. Do not delete them, and do not open or edit `.pen` files.

## Read first

- Today's surfaces: `crates/runner-app/src/surfaces/panes/start.rs` (`render_start_surface`), `panes/mod.rs` (`start_surface_visible`), `panes/pane.rs` (the empty-pane “Open in this pane” rows).
- The form: `surfaces/start_chat/` — `modal.rs` (every `open_*_chat_modal` entry), `launch.rs` (`submit_start_chat`, `build_start_request`, `ChatLaunch`), `mod.rs` (recent runtimes file, `initial_direct_runtime`), `terminal.rs` (`new_terminal_action`, `terminal_start_location`, `spawn_terminal_in_pane`), `add_role.rs` (stays).
- Entry points: `sidebar/view.rs` and `sidebar/menus.rs`, `command_palette.rs`, `front_page/folder_picker.rs`, `roles/list.rs` and `roles/menu.rs` (`start_role_chat`), `app_shell/mod.rs`, `keymap.rs`.
- Remote: `front_page/client.rs` (capabilities, `launch_chat`), `remote_client/`, `front_page/folder_picker.rs`.
- Reuse `ProjectField`, `WorkingDirField`'s pickers, `RoleAvatar`, `runtime_mark`, `filter_selectable_runtime_catalog`.

## Deliverables

Land the spec's three phases, each commit building and passing its tests: the launcher model with the recency store and its migration; the launcher surface with every entry point, start action and the remote window; then removal of the old picker, start-page rows and the form's chat path (⌘1/⌘2 go too; `add_role.rs` keeps the form), with the guide pages, architecture and regression rows the spec lists, English and zh-CN together.

Decide undrawn cases by the nearest drawn state and list each in the PR: for example a pane too narrow for the 400 px column, very long role names, a role with its own directory after Change, or the remote launcher while the host's catalog loads.

Out of scope: a per-chat terminal button, starting an agent by typing its command in a shell, the add-role-to-mission form, Start mission's form, daemon, protocol or schema changes, `.pen` files and unrelated refactors.

## Validation

Before running checks, list the changed behaviours and the check covering each. Expected: `runner-app` tests for ranking, fill, recency migration and per-computer lists, keys, every entry point, start requests and remote capability hiding; workspace Clippy with `--profile ci`, `cargo fmt --all --check` and `git diff --check`; `make guide-cli` only if CLI help changes. Coder and reviewer never run the app.

After the clean code review, `qa` runs these live checks in the development app under AGENTS.md Test Scope. First create a bounded test role (an inexpensive model, low effort) and use it for any prompt; agent rows start with the agent's own configuration, so start them only to observe the launch and send them no prompt. Use fresh `/tmp` folders and archive only what qa created.

- The default page in a window with no tab, and after closing a tab's last pane.
- ⌘N from a running chat opens a new tab with the launcher; typing finds the test role; ↵ starts it; matched letters are highlighted.
- ⌘D beside that chat: the launcher shows the neighbour's folder; Terminal starts a shell there.
- On the default page: the project picker and Change set where the next chat starts.
- Recent order after starting two different entries; 1–4 with an empty search; digits go into a non-empty search; Esc clears it.
- No Start a chat form appears from ⌘N, the sidebar, a project row or the palette; the role page's Chat still starts directly.

`qa` records results in `docs/tests/1092-chat-terminal-launcher.md` with screenshots outside Git, and marks anything not run with its reason. Remote windows and Windows/Linux native checks are left to Jason. A failed check goes back to `coder` with evidence.

## Roles and handoffs

`coder` implements and runs the checks, then hands a frozen snapshot of the uncommitted diff to `reviewer` through Runner. `reviewer` posts must-fix findings first with file:line pointers; iterate until clean. The clean review confirms the live checks or names changes. The orchestrator then reviews the working-tree diff and replies with findings or “go” through Runner; wait for it. `coder` then commits, pushes and hands the SHA to `qa`; after qa passes, `coder` folds the test record into the commits, opens the PR and drives CI.

## Authorization

Jason authorized this `codex trio` mission (coder, reviewer, qa) with implementation, validation, commits on this branch, pushing it and opening one PR against `main`, then driving the required CI green on every platform. No other agents, missions or internal subagents.

Fold this brief and the spec into the commits; fold later fixes into the commit they correct and push with `git push --force-with-lease`. Rebase onto `origin/main` only when AGENTS.md requires it; never merge `main` into the branch. No tool-specific co-author trailers. The PR closes #1092, takes `frontend-only` if the diff stays in `runner-app`, keeps `core-review` if the workflow adds it, takes one risk label (expect `risk-high`), and lists the live checks run and those left to Jason.

Stop at an open PR with green CI and report through Runner with the PR URL, final SHA, changes and validation. Do not merge, release, cut a nightly, archive the mission, delete branches or remove worktrees. When waiting, finish the turn and rely on the inbox notification; do not poll.

## Authorized follow-ups — 2026-10-31

Jason added #1093 to this mission as a separate `fix(ui)` commit: restore the six-dot pane drag grip before the runtime mark in split tabs, keep it in narrow headers, and let Runner own macOS titlebar dragging. Follow `target/design-ref/pane-drag-grip.png`; preserve drag previews and drop rules. Update the island spec, pane-moving guides in both languages, and WS-PANE-03. QA checks a top-row grip moves the pane without moving the window; empty island headers, the top margin, sidebar head and side-panel head still move the window; header double-click still zooms; single-pane tabs have no grip. Windows and Linux are left to Jason. The PR also closes #1093.

Jason also authorized a separate `fix(ui)` commit making all terminal closes immediate: pane ×, the focused-terminal close shortcut, and sidebar terminal-tab close, locally and remotely, including a foreground command. Remove the obsolete terminal confirmation targets/strings and update tests and both-language guides. Chat archive confirmations and archive-all counts stay unchanged. QA closes a terminal running `sleep 100` by each entry point without a dialog and verifies that archiving a running chat still asks.

Both follow-ups use the same frozen-snapshot code review, orchestrator go and QA gates. They add no agents or missions, and keep the open-PR/green-CI stopping point and all merge/release/cleanup prohibitions above.

The orchestrator approved combining #1092 phases 2 and 3 into one reviewed commit on 2026-10-31 under AGENTS.md’s crew-shaped PR history. The launcher model, #1093 and immediate terminal close remain separate commits.
