# #988 — WSL2 agent execution validation

Date: 2026-10-28. Branch: `feat/988-wsl2-agent-execution`.
Worktree: `.worktrees/feat-988-wsl2-agent-execution`.

## Scope

Selected checks cover target persistence and inheritance, path conversion, the CLI
bridge, frozen resume/fork definitions, the added selectors, and packaging inputs.
They do not constitute a full regression or a Windows + WSL2 feature pass. The user
requested implementation first and will perform Windows environment checks on a
corresponding machine. No Windows app, WSL distribution, Docker environment or
real agent session was launched for this change.

## Local evidence

| Check | Result |
| --- | --- |
| `cargo test --offline -p runner-core -p runner-daemon -p runner-cli -p runner-app --lib --bins` | Passed: core 66; daemon 1,274; CLI library 9 and binary 71; app library 155 and binary 561. Total 2,136 tests. |
| Additional pure Linux/WSL UNC path conversion test | Passed; preserves case and rejects another distribution. |
| `cargo clippy --offline --workspace --all-targets -- -D warnings` | Passed, including the final code changes. |
| Final core/CLI Clippy with the `wsl-client` build feature and restored manifests | Passed. |
| Linux build script / workflow YAML syntax | Passed (`bash -n` and Ruby YAML parser). |
| Generated CLI reference `--check` | Passed; generated from the built CLI's help output. |
| Formatting and patch whitespace | Checked during final cleanup. |

The local macOS machine lacks the Metal compiler. To run the app's unit tests and
workspace Clippy, a temporary macOS dev dependency enabled GPUI's `runtime_shaders`;
the original Cargo manifest and lockfile were restored afterwards. This establishes
Rust compilation and unit behavior, not the production Metal shader build.

Earlier daemon test attempts hit sandbox-denied Unix sockets; the authorized
unsandboxed unit run above passed. The migration snapshot was stale and the
historical migration-28 test compared against all later migrations; both were
corrected before that passing run. The subsequent CLI integration/golden run hit
the same sandbox socket restriction and was stopped, not reported as passing.
Help golden text was regenerated directly from help output; additive native target
JSON fields were updated from the serialization contract. Full golden execution
remains unverified. A Windows core cross-check stopped at unavailable offline
dependencies and was not completed. No further environment checks were attempted
after the user's clarification.

## Windows handoff (not executed)

Use the development app and its separate data. Preserve existing sessions; only
operate chats and missions created for this validation. Use tiny prompts and a
fresh test repository.

For a local development build, build the Linux CLI with `bash script/build-wsl-cli`
in Linux/WSL, then copy `target/wsl/runner-agent-cli-linux` beside the Windows
`target/debug/runner-agent-cli.exe` before starting the development app. Build both
binaries from this branch with matching `RUNNER_BUILD_SHA` and `RUNNER_BUILD_STAMP`
(or leave both unset for a local build). Packaged nightly/release workflows obtain
the Linux binary automatically. The development integration installs `runner-dev`;
production installs `runner`.

1. **Integration:** Settings → Agents → WSL2 lists distributions and rejects WSL1.
   Enable the default user, then an explicit other user. Confirm detected Linux
   CLIs and Linux homes. A foreign `~/.local/bin/runner[-dev]` must be preserved.
2. **CLI:** in a normal Linux terminal, add `~/.local/bin` to PATH if needed. Run
   `runner-dev status`, role/crew/project commands and `runner-dev chat start`.
   Confirm the same Windows daemon/database is used. Disable WSL interop or move
   the managed bridge temporarily: commands must fail without a Linux daemon.
3. **Paths:** launch in a Linux repository with spaces and Chinese characters,
   a symlinked repository, and a Windows disk repository. Test same-spelling paths
   in two distributions and case-distinct Linux directories. Cross-distribution
   UNC paths must fail clearly; ordinary UNC links open the intended host file.
4. **Lifecycle:** stop, resume, fork, restart a slot, restart the daemon and shut
   down the distribution. A stopped session leaves no Linux agent/tool descendants,
   including tools that use `setsid`. Other sessions remain alive. Normal exit codes
   are retained; a missing host lease cleans children. Altered PID generation or
   boot identity must never signal a different process.
5. **Runtime files:** check a selected Linux user's sign-in/configuration, system
   prompt, trust files, resume key and transcript/status hooks. Edit a saved role's
   target afterwards: the old session must retain its frozen target.
6. **Mission:** all initial and dynamically added members use the mission target.
   Select the applicable live regression cases `MIS-ADD-01`, `MIS-MSG-06`,
   `MIS-ASK-01`, plus `RT-LAUNCH-01` for resume. Verify `msg post/read`, `signal`,
   `ask`, `--goal-file -` and `mission feed --follow --json`. The Windows NDJSON
   log remains authoritative, with no duplicate events.
7. **Update/disable:** use a mismatched CLI build and confirm an update message;
   start WSL work after a matching Windows update to refresh the helper. Disable
   integration after stopping its sessions, preserving foreign commands and agent
   data. Native Windows launches and existing records still use Native by default.

Linux CI includes a musl CLI build and launcher process ownership tests. Remote CI
results after the authorized push belong to the pull request; they are not included
in the local evidence above. Windows + WSL2 live checks remain assigned to the user.
