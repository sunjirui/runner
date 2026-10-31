# 748 — Link a chat to the missions it starts

> Tracking issue: [#748](https://github.com/yicheng47/runner/issues/748). Priority: P0, milestone 0.18. Platforms: macOS, Windows and Linux.
> Status: draft, 2026-10-27; merged to `main` as a spec on 2026-10-30, ahead of its code. The surface needs a design before implementation; see [Design](#design) and [Decisions to confirm](#decisions-to-confirm).
> Replaces the watch-delivery proposal (2026-09-29, refreshed 2026-10-17), on hold since Jason rejected Runner typing mission updates into a normal chat; see [Superseded design](#superseded-design).
> Related: #686's watch contract ([test record](../tests/archive/686-watch-cli-started-missions.md)), which this retires; [704](https://github.com/yicheng47/runner/issues/704), which first specified `RUNNER_SESSION_ID` and closed unbuilt; [#701](https://github.com/yicheng47/runner/issues/701)'s desktop notifications, which already report the states this shows; [873](https://github.com/yicheng47/runner/blob/2a22742e0b89e1a8711c858c0b11532cb04497f4/docs/features/873-remove-mission-lead.md), which removed the mission lead; [#916](https://github.com/yicheng47/runner/issues/916)'s verbs.

## Motivation

A chat is Jason's frontman. He talks with it about anything, and it delegates work by running `runner mission start` and `mission add-role`. By design it is not a member of the missions it starts. Nothing links them: the mission does not know which chat started it, and the chat does not show its missions.

The generated skill fills the gap by making the agent watch. Rule 5 requires every external operator to arm `runner mission feed <id> --follow --json` in a background facility that wakes it while idle, and rules 6–7 cover re-arming, expiry and the "automatic watching is unavailable" fallback. That is too heavy:

- **Codex cannot do it.** It exposes no tool that brings a background process's output into an idle conversation, so a Codex chat reports that watching is unavailable and Jason comes back to ask. The chat that started #724's mission did exactly that on 2026-09-28.
- **Claude Code can, at a cost.** Monitor keeps one follower process per mission, every event it surfaces wakes the chat for a model turn, the feed fills the conversation's context, and the skill spends three rules on keeping the watch alive.
- **Since #873 there is no lead** ([#1049](https://github.com/yicheng47/runner/pull/1049), 2026-10-27). The chat that starts a mission is its only coordinator, so the missing link matters more.

Runner already watches every mission. The daemon sees each event, and the app already raises desktop notifications when a mission asks a question, a member waits for input, a member fails, or every member is idle (`crates/runner-app/src/notifications.rs`). What is missing is the link between the chat and its missions, and a place on the chat that shows them.

## Direction

Jason, 2026-10-18 and 2026-10-27:

1. **A link, not membership.** When a chat runs `runner mission start`, Runner records that chat as the mission's starter. The chat stays outside the mission, and its conversation is unchanged.
2. **Runner tracks the mission; the agent does not watch.** The agent reports that the mission started and finishes its turn. No follower, no Monitor, no polling.
3. **Updates go to the person, not into the chat.** The chat's surface and the existing desktop notifications tell Jason when a mission needs him, fails or goes idle. Runner never types into the chat on its own (rejected 2026-10-17). The frontman catches up when Jason asks it.
4. **Independent lifecycles.** Archiving the chat leaves its missions alone, and archiving a mission removes it from the chat's list.

**Tradeoff accepted:** nothing automatically carries a crew's result into the frontman, so the chat does not react when a crew finishes, for example by starting a review once a PR opens. Jason stays in that loop.

## Scope

### Identity: `RUNNER_SESSION_ID`

Every agent session gets its own session ID in `RUNNER_SESSION_ID` on spawn, resume and fork, as 704 first specified. Today direct chats export only `RUNNER_HANDLE` as a label, and only runtimes with Runner hooks carry `RUNNER_HOOK_SESSION`, which belongs to the hook protocol (`runtimes/helpers.rs::hook_env`). The variable is an address, not a caller: a chat still acts for the person (arch §9.3), and a direct chat stays off the bus (arch §9.4). The session ID is stable for the chat's life, since resume and restart reuse the same row (arch §5.3 and §5.5).

The variable must reach the `runner` the agent's tool shell runs. Mission slots do not rely on the PTY's environment for this: a per-session shim bakes their variables into `runner` (`cli_install::install_session_runner_shim`), because Claude Code's Bash tool has run commands in a shell without them. Phase 1 checks whether Codex and Claude Code pass a direct chat's environment through. If either does not, direct chats get the same kind of shim, keyed by session ID.

### The link

- The `mission_start` request takes an optional `started_by_session_id`, which the CLI fills from `RUNNER_SESSION_ID`. The daemon records it only for a direct agent chat. It ignores a terminal pane and a mission slot, and the start succeeds either way.
- It is stored as a nullable `missions.started_by_session_id` column, added by one migration. Deleting the chat clears it; archiving either side keeps it.
- It is set once, at start. `mission resume`, `add-role` and later commands from any chat leave it unchanged. A mission started in the app, or by an agent outside Runner, has no starter.
- The local and companion protocols carry it additively as `started_by: {session_id, title, archived}` or null on mission summaries and details, so remote windows show the same link and clients of other versions still parse.

### Mission state on the chat

The chat shows each live mission it started with one state and one line, derived from what the notifications already compute:

| State | When | Line |
| --- | --- | --- |
| **Needs you** | A `human_question` is pending, or a member waits for input or approval. | The question, or the waiting member. |
| **Failed** | A member crashed or exited with an error. Other members may still be running. | The member and its exit. |
| **Working** | A member is working. | The latest broadcast. |
| **Idle** | Every member has finished its turn and no question is pending: done or stuck. | The latest broadcast, which says which. |

The first state that applies wins, top to bottom. A stopped mission, or one whose sessions have all exited, collapses into an Ended group; an archived mission leaves the list. Idle is the "done" signal: no `runner done` verb is needed for this.

Desktop notifications keep their current triggers, settings and targets. Clicking one still opens the mission.

### Design

Two surfaces are to be drawn for comparison in `runner/desktop/specs/748-coordinator-missions.pen` in the design repo, beside the 2026-10-20 panel options:

- **A. Badge and strip (recommended).** The chat's sidebar row carries a small badge for its most urgent mission, such as "1 needs you". Above the chat's terminal, one line per live mission shows the state, the title and the line from the table; clicking it opens the mission.
- **B. Panel.** A Missions view in the chat's right side panel lists the chat's missions with the same fields. It pays off only when one chat runs several missions at once.

On the mission side, a "Started from <chat>" caption links back to the chat, and reads "(archived)" when the chat is.

### CLI

- `mission start` JSON gains `started_by`. Plain output, `-q`, stdout and exit codes are unchanged, and `mission start` still returns at once.
- `mission show` gains a `STARTED BY` line and the `started_by` JSON field.
- `mission list --started-here` lists the missions this chat started, read from `RUNNER_SESSION_ID`. Outside a Runner chat it exits with a hint. It lets the agent find its missions after its context is compacted or cleared.
- `mission feed --follow` stays, for people and scripts that want a live tail.

### Skill, help and docs

`crates/runner-core/src/protocol/agent_skill.rs` (both the `runner` and `runner-dev` renders) and `crates/runner-cli/src/help.rs` change together:

- **Rules 5–7 are removed:** start-and-watch, the watch's surfacing, cleanup, expiry and re-arm rules, the qualifying-facility test, the Monitor recipe and the "automatic watching is unavailable" line.
- **A new rule:** after `mission start` and `add-role`, tell the person the mission ID and that Runner shows its progress, then finish the turn. Do not start a feed watcher, poll or sleep-loop for a mission you started. When the person asks for an update, use `mission list --started-here` to find your missions and `mission show <id>` and `mission feed <id>` to read them. Summarize briefly, relay a pending question to the person, and answer it with `mission answer` only with their decision.
- The rule is the same for agents outside Runner. They have no link, and the person follows the mission in the app.
- Skill and help tests pin the new rule and the absence of the watch contract for both renders.

Docs:

- **Guide:** [Drive Runner from agents](../guide/guides/drive-runner-from-agents.md) loses the external watch contract in "Start and follow a crew" and gains the link and catching up. [Missions](../guide/concepts/missions.md) describes the starter link. The page for the chosen chat surface is named once the design is chosen. `make guide-cli` regenerates the [CLI reference](../guide/reference/cli.md).
- **Architecture:** `docs/arch/arch.md` §9.4 documents `RUNNER_SESSION_ID` and the link, and §9.6 replaces the skill's start-and-watch contract.
- **Regression:** [MIS-WATCH-01 and MIS-WATCH-02](../tests/regression/missions.md) retire, replaced by rows for the link and the agent that does not watch.

## Decisions to confirm

1. **Catching up is pull-only** (recommended). Jason asks the chat, and it reads the mission with `mission show` and `feed`. The alternative is a Catch up button that Jason presses to type one pointer line into the chat; it can follow later if he misses it.
2. **The surface is the badge and strip** (recommended), or the panel. Either way it lists only the missions this chat started, not the project's other missions.
3. **A mission links back to the chat that started it** with a "Started from" caption (recommended).
4. **One rule for every host.** The watch contract goes for agents outside Runner too, so the skill has one rule (recommended). The alternative keeps rules 5–7 for agents outside Runner.
5. **Only a direct agent chat is recorded as a starter.** A mission slot that starts another mission records nothing, and a terminal pane cannot be one.

## Out of scope

- **Typing anything into the chat automatically.** An opt-in "tell the chat when this mission finishes", off by default, can follow later.
- **Linking an existing mission to a chat, or moving the link**, such as a `mission link` command.
- **#916's verbs**, including `runner done` and `wait`.
- **Codex outside Runner** through its app server.
- **Jason's personal `mission-watch` skill**, and his standing rule that Claude Code arms a watcher after every mission start. Both are updated outside the repo when this ships.

## Superseded design

The proposal this replaces made the starting chat a **watching chat**: the mission's dispatcher typed one-line `[Runner] Mission …` notices into it when it was idle, with the readiness holds, confirmation and redelivery of arch §8.5, and the agent answered each notice in a new turn. Jason rejected the mechanism on 2026-10-17 because the chat is his conversation, and on 2026-10-27 dropped agent-side watching altogether. Its findings still explain why the agent cannot watch: Codex has no idle-wake facility, a Stop-hook or `write_stdin` loop keeps the chat's turn open for the whole mission, and the Codex app server cannot reach a chat that Runner hosts. The full text is in git history as `docs/features/748-mission-watch-delivery.md`, last at `9e5ba05e`.

## Implementation phases

1. **Backend.** `RUNNER_SESSION_ID` for every agent session, with a shim for direct chats if a runtime drops it; the migration and `started_by_session_id` on start, with its validation; `started_by` on the local and companion protocols; the `--started-here` query. Tests: a chat is recorded, while a terminal pane, a slot and a missing variable record nothing; resume and `add-role` from another chat leave it unchanged; deleting the chat clears it; an old database migrates.
2. **App.** The state derivation, the signed-off surface and the mission's back-link. Tests: the state precedence, the Ended group and removal at archive, and a chat with two missions.
3. **CLI, skill and docs.** The JSON fields, the `STARTED BY` line, `--started-here`, the skill and help rule, the guide, arch and regression rows.
4. **Live checks**, under the standing development-app authorization in `AGENTS.md`; see Verification.

After the design is signed off, the phases are one `codex-trio` mission, since the surface needs live checks by its QA slot.

## Verification

### In-process

- Database, daemon and CLI tests for phase 1, including `mission show` and `mission list --started-here` output, JSON and golden files.
- App tests for the state table and the surface.
- Skill and help tests for both renders; `make guide-cli`; workspace clippy.

### Live, macOS

Run in the development app with an inexpensive model, low effort, tiny prompts and fresh folders under `/tmp`.

- [ ] A fresh Codex chat with only the installed skill, asked to start a small mission that posts a handoff, asks a yes/no question and finishes, returns at once, tells the person the mission ID, starts no follower and ends its turn.
- [ ] The chat's surface follows that mission from Working to Needs you and to Idle, with the line from the table each time, while nothing is typed into the chat. Clicking it opens the mission, and the mission's caption links back.
- [ ] Killing one member shows Failed while the other keeps running.
- [ ] Asked "how is the mission going?", the chat reads it with `mission show` and `feed` and summarizes it. After `/new` in the same chat, `mission list --started-here` still finds it. The chat answers the question only with the person's decision.
- [ ] Two missions from one chat both show. A mission started from the app shows no starter. Archiving the chat leaves its mission running with an "(archived)" caption, and archiving the mission removes it from the chat.
- [ ] Claude Code in a Runner chat behaves like Codex and arms no Monitor.

Windows and Linux use the same daemon and surface and stay unverified until checked.
