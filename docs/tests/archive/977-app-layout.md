# #977 app layout — validation

Scope: the app-only pure moves on `refactor/977-app-layout`, based on `4bc32f5a`. The selected checks and responsibility boundaries were recorded before implementation in `docs/impls/977-app-layout.md`. No original function or test body changes were found by the parser/token audit; only two entry-point wrappers were added. No assertions or integration-test source files changed. Inline test modules move intact, with new logical paths for the root and settings-page tests.

## Automated checks

| Scope | Check and result |
| --- | --- |
| Single module tree, root/window/startup moves and public library paths | Complete `cargo test -p runner-app`: 939 unit tests and 37 integration tests passed; three existing opt-in runtime smokes remain ignored. |
| Individual surface boundaries | Production `cargo check -p runner-app` and focused app tests at each split: panes 35, settings page 21, start-chat 53, chat 10, shell 25, agents 29 and remote client 73 passed. |
| Workspace and platform-gated code | `make clippy`, including updater, passed. Windows and Linux compilation are left to CI. |
| Required workspace gate | `make fmt`, `make verify` and `git diff --check` passed. |
| Move purity and publication boundaries | Eight ordered patches reconstruct source candidate tree `3220a27289aaa7c6e5b6e4357bd3089cf299536c` exactly; each source move was checked before the next. The initial mission brief will be folded into the first move commit after clean working-tree review. |

The full successful local app and workspace runs used a command-local `ulimit -n 65536`. This permits validation to finish; it does not resolve the default-limit failure documented below. No global configuration changed, and the native app was not launched. Live checks are determined by the reviewer under Test Scope.

## Default descriptor-limit comparison

Jason requested this gate after the initial raised-limit runs. Both snapshots were tested with `cargo test -p runner-app` at the inherited soft limit of **10,240**, with `RUST_TEST_THREADS` unset and 18 logical CPUs. Both used this worktree's own Cargo target directory and `CARGO_BUILD_JOBS=12`. Source files were temporarily restored to the exact base in this worktree, verified against all 229 base app file blobs, then restored exactly to the staged candidate; no additional checkout was created.

Numeric file descriptors were sampled per test process with `lsof -nP -a -p PID -F f`, counting only records matching `^f[0-9]+$`. The monitor sampled continuously at approximately 0.1–0.3 seconds per active process. These are sampled peaks, not guaranteed instantaneous maxima. An earlier informal count included memory mappings and is not used as descriptor evidence.

| Snapshot and test process | Result at the default soft limit | Sampled peak numeric descriptors | Samples |
| --- | --- | ---:| ---:|
| Base `4bc32f5a`, library test binary | 168 passed | 54 | 6 |
| Base `4bc32f5a`, app binary test harness | 770 passed, 1 failed, out of 771 | 10,000 | 268 |
| Candidate source tree `3220a27`, library test binary | 938 passed, 1 failed, out of 939 | 10,000 | 265 |

Both complete commands exited 101, without a timeout. Both failed `surfaces::start_chat::tests::confirm_binding_precedes_every_control_and_preserves_open_menu_values` while opening its temporary SQLite pool, with `Pool(Error(Some("unable to open database file")))`. Later integration suites were not reached after the unit-test failures. The candidate's binary harness has no tests; the library now contains the base app binary's 771 tests plus the base library's 168 tests.

The base command took 48.27 seconds and the candidate command 46.68 seconds under monitoring. Machine-local evidence is in `/tmp/977-app-layout-{base,candidate}-fd.json`, the corresponding `-default-fd.log` files, and `/tmp/977-app-layout-measure-fd.py`. The operator independently observed the base app failure twice in the parallel CLI/daemon mission; the base's existing `docs/tests/977-remote-root.md` also records command-local descriptor-limit increases from earlier work.

This establishes a **pre-existing default-limit failure**, rather than a new failure introduced by consolidating the test modules. Both runs reach the same sampled peak; this comparison does not measure an unconstrained descriptor peak or identify a retained-owner leak. Jason assigned descriptor cleanup to a separate follow-up. No resource-lifetime fix or alternative test layout is added to this pure-move PR. The default-limit `make test` / `make verify` limitation remains visible.

The earlier default-limit candidate run and its repeat each failed one test after 938 passes; an isolated rerun passed. A serial diagnostic run at the same default limit failed three SQLite-open tests after 936 passes. Both complete app runs and the required workspace gate subsequently passed with the command-local 65,536 limit. These earlier failures are retained as diagnostics, not described as successful default-limit validation.

## Production line counts

Count physical Rust lines, including production comments and blank lines, excluding test-only files and inline items whose `cfg` gates cannot compile with `test=false`. Mixed `any(..., test)` branches remain counted when they can compile in production. Apply the same parser-based method to base and candidate.

App production files over 1,000 lines decrease from **22 to 15**. `main.rs` decreases from **2,087 to 5** production lines. The largest new production module is `surfaces/settings/page/mod.rs` at **1,118** lines. The only remaining file above approximately 1,500 is unchanged, out-of-scope `terminal/element.rs` at **1,515** lines. The PR contains the complete before/after table for every touched Rust path.

## Regression maintenance, 0.19.0

None apply: this was a behavior-preserving module restructure. Its automated checks stay with the owning tests, and no live case was added.
