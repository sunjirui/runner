# #988 — WSL2 execution implementation plan

Tracking issue: [#988](https://github.com/yicheng47/runner/issues/988).
Product decisions: [feature specification](../features/988-wsl2-agent-execution.md).

## Contract

The Windows daemon remains the only owner of sessions, SQLite and mission logs.
An execution target is either `native` (the default for existing records) or
`wsl`, identified by distribution and Linux user. Targets are copied into launch
definitions so resume and fork cannot follow subsequent role edits. A mission
has one target, inherited by its initial and subsequently added members.

Linux paths must retain their case, spelling and distribution identity. Convert
paths with the selected distribution's `wslpath`; never guess its drive mounts.
Host file access uses WSL UNC paths, while agent arguments use Linux paths.
Argument and environment values are passed as separate process arguments.

Use the current versioned client frames over a stdio bridge to the existing
Windows named pipe. A Linux CLI in integrated mode must fail closed when its
bridge is unavailable, without falling back to a local daemon. Mission log
operations execute on Windows, with Linux file inputs resolved before crossing
the boundary. No additional network listener is needed.

## Implementation order

1. Define and validate execution targets, WSL discovery, and path boundaries.
   Add persistence with backwards-compatible defaults and freeze session targets.
2. Add bounded WSL command execution, per-session Linux process ownership,
   startup acknowledgement, and graceful stop with a kill backstop. Preserve
   the existing native PTY path.
3. Add the Linux CLI bridge and integration install/remove operations, including
   version checks, collision handling, and development/production separation.
4. Wire target selection and distro-specific agent discovery into the existing
   role, chat, mission and Settings controls. Route agent state reads and hooks
   through the session's target. Update the installation and agent guides.

## Selected validation and reasons

| Changed behavior | Checks | Reason |
| --- | --- | --- |
| Target identity and persistence | Core serde/path tests; daemon repository and launch-definition tests | Old rows must remain native; role edits must not retarget an existing session. |
| Process launch and termination | Launcher argument tests; Windows + WSL2 lifecycle checks | Windows process termination alone does not prove Linux descendants exited. |
| CLI bridge | Fragmented/binary frame tests, EOF, version mismatch and disconnect checks | Feed and requests share a bidirectional connection; Linux fallback must be impossible. |
| Mission coordination | Target inheritance and host log ownership tests; `MIS-ADD-01`, `MIS-MSG-06`, `MIS-ASK-01` | Added roles, replies and asks must keep one authoritative log and target. |
| Resume/fork | `RT-LAUNCH-01`, targeted adapter and resume tests | Conversation files and frozen launch definitions live across the OS boundary. |
| Native UI | runner-app tests and workspace Clippy; Windows Settings/start controls | Target selection changes the launch UI and must surface unavailable distributions. |

Run formatting and the relevant core/daemon/CLI tests as implementation proceeds;
run runner-app tests and workspace Clippy after UI integration. Windows + WSL2
checks require a real Windows host. macOS tests cannot establish ConPTY, WSL
interop or Linux process cleanup. Record that gap explicitly; do not call it a
feature pass or a complete regression pass. No full smoke test is requested.

## Implemented boundaries

- Migration 0029 adds native defaults to existing roles, projects and missions;
  launch-definition JSON retains a backwards-compatible target default.
- Linux helpers prepare runtime arguments, trust and hooks in the Linux user's
  environment. Windows watchers receive explicit UNC file roots. Cursor's Linux
  process file observation crosses a per-launch state file; hooks use the existing
  framed transport.
- Each launcher is a Linux subreaper, identifies PID generation and boot identity,
  and owns its descendants. A Windows heartbeat lease provides a daemon-loss
  cleanup path. Startup errors also run the descendant cleanup guard.
- Integration installation uses an owned wrapper, atomic replacement and a
  collision check. The wrapper carries its installation root so a release-built
  Linux helper can serve the isolated Windows development channel too.
- Windows builds obtain a static Linux x86_64 CLI artifact from Linux CI. The `wsl-client` build refuses to start a daemon even when invoked without its
  wrapper. No Linux desktop application or daemon is started by the integration.

Local automated results and the user's Windows handoff are in the
[validation record](../tests/988-wsl2-agent-execution.md).

## Delivery

Worktree: `.worktrees/feat-988-wsl2-agent-execution`.
On 2026-10-30, the user authorized changing `origin` to
`git@github.com:sunjirui/runner.git`, committing and pushing this branch, and
opening a pull request against that repository's `main`. Merge and release remain
unrequested. The original staged documents in the root checkout stay untouched.
