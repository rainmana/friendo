# Local verification and CI

The required preflight is:

```sh
bash scripts/check.sh
```

It runs `cargo fmt --all --check`, `cargo clippy --all-targets --locked -- -D warnings`, `cargo test --all-targets --locked`, `cargo test --doc --locked`, `cargo build --locked --bins`, and the Unix terminal helper on Linux/macOS. Cargo compiles what each command needs; a build or `cargo test --no-run` alone does not execute assertions. There are no application-defined Cargo features to multiply into a feature powerset matrix.

The released v0.1.0 base was verified on macOS/aarch64 with 65 passing Rust tests: 20 library unit, 7 configuration, 5 real CLI, 18 native process, 10 provider, and 5 runtime integration tests. These counts describe that base, before the local Chat/Expert additions. Current UI validation belongs in [build status](build-status.md); the new regression targets below do not establish that checks have completed. Doctests are checked, with zero examples currently defined. Provider and replay-sentinel tests need local loopback networking. No hosted credential is needed for preflight.

## Covered boundaries

- Policy: provider credential names cannot be either the destination or source of an environment override; ordinary/missing references, deadline rejection/clamping, malformed arguments, shell-disabled dispatch, and unsupported tool fields are exercised. Rejected calls must not append `ToolStarted`.
- Storage: complete corruption, invalid UTF-8, unknown event type/schema, broken sequence, mixed run identity, oversized records, and failed actual writes fail without applying the proposed transition. Artifact tests cover canonical path containment, sibling-prefix escape, missing/oversized/non-text files, and Unix symlink escape.
- Configuration: partial defaults, provenance for explicitly supplied default values, unknown fields, numeric boundaries, remap collisions/reserved keys, exact executable paths, explicit wildcard admission, and YOLO's unchanged admission policy.
- CLI: real subprocesses use explicit temporary config/data and bounded waits. Tests exercise `doctor`, defaults, precedence, credential readiness/secrecy, invalid input, synthetic demo, listing, replay prefixes, and exact context. Replay compares all retained file bytes and checks HTTP/command sentinels, including an uncertain action and damaged tail.
- Processes: real fixture processes, descendants, cancellation, output bounds, sessions, redaction, and deadlines remain covered. Deadline observation uses completion waits rather than a fixed post-deadline sleep. Windows assertions check empty Job Object evidence and descendant exit; a Windows-only test checks unsupported pipe interruption followed by observed termination. Drop/orphan cleanup checks also compile for Windows.
- Terminal: the standard-library Python helper runs the real Rust executable in a Unix PTY. It waits for durable events and visible cell states, rather than scheduling keys at fixed elapsed times. Its small screen observer handles the cursor/erase/style subset emitted by Ratatui; it is QA code, not product process-terminal emulation.

The released base's terminal helper ran two isolated journeys. The synthetic demo checked repair, NORMAL/YOLO, resize, and historical controls. Live mode used an explicit loopback fixture, one real harmless fixture command, exact journal-to-model tool results, exact retained HTTP bodies, Unicode bracketed paste, context inspection, remapped controls, constraint retention, correction against a held third response, then Ctrl-C against a separately held fourth response. The two canceled streams must never become final answers. Real replay verified the final reducer state. Both journeys verified exit zero and terminal-mode/cursor/alternate-screen/bracketed-paste restoration. The local UI changes retain this Expert coverage and add Chat-specific checks plus a separate read-only replay terminal with no-color/effects-off, F2 switching, blocked Ctrl-C, terminal restoration, and retained-file byte equality; their completed results are recorded separately in build status.

Generated captures are `docs/captures/demo.cast`, `docs/captures/live-fixture.cast`, and `docs/captures/chat-replay.cast`, ignored by Git. `FRIENDO_BIN` and `FRIENDO_FIXTURE_BIN` can select instrumented binaries. Temporary runs/configs never consult personal `friendo.toml` or existing runs. This is fixture-backed live-mode evidence, separate from [the real Anthropic verification](live-anthropic.md).

## Chat and Expert regression targets

The source build defaults to Chat, with a focused composer and a compact live map. Expert remains available through `--ui-mode expert` or F2. Existing Expert terminal journeys must select Expert explicitly so a shortcut such as `r` does not accidentally become composer text. The current shared `friendo.toml` stays compatible with installed v0.1.0 by omitting `ui_mode`; the new build supplies Chat as a default. CLI configuration checks exercise built-in/file/CLI precedence and validation without contacting a provider.

Chat verification should exercise the actual Rust process with the same isolated HTTP/process fixtures as Expert. Type ordinary `q`, `y`, and `r` in focused input, paste Unicode and newlines, submit, browse with Esc, return to typing with `i`, and switch modes with F2 while preserving the draft, active work, history position, and autonomy. F3 inspects selected cards/evidence, F4 opens correction, F5 opens retained context, F6 focuses the map or returns to the conversation, and PageUp/PageDown scroll conversation or inspectors. Displayed bindings must remain usable at narrow sizes; narrow Chat retains a compact map/activity area above the conversation.

Pure projection/render tests and captured CLI replay should check exact user statements, explicit correction attribution, matching ToolStarted/ToolFinished identities, stale earlier answers, disposable streaming preview, and historical missing completion as **outcome unknown**. Read-only replay must have no active composer or mutable controls, and inspection must leave retained files byte-identical. Understanding must distinguish recorded intent, model interpretation, and observed evidence without inventing private hidden reasoning, acknowledgment, confidence, or an unrecorded plan. Switching presentation must not append events, dispatch tools/models, or change NORMAL/YOLO or admission policy; both modes consume the same journal.

Use the normal preflight to validate the integrated build, then inspect actual wide and narrow rendered frames. Record executed platforms, fixture versus hosted checks, failures, and unresolved limits in build status rather than transferring the released base's results to the new UI.

## CI policy

[ci.yml](../.github/workflows/ci.yml) runs for PRs, pushes to `main`, and manual dispatch. Feature-branch pushes without a PR do not consume a second hosted run. Workflow concurrency cancels obsolete runs for the same PR/branch. No path filter can strand a required status on a docs-only PR.

Linux runs the full preflight first. macOS and Windows run after that gate, retaining native Clippy for platform-specific `cfg` code and native tests on all three operating systems. Unix runners also execute the PTY helper. Tests are not retried automatically. Jobs have bounded timeouts; failures upload logs and credential-free terminal captures for seven days. Existing Rust caching remains.

The real CLI integration tests replace standalone headless-demo/doctor exit-zero smoke commands. Native tests still matter: compiling Windows code cannot prove Job Object behavior, and fixture HTTP cannot prove hosted model support. The released base's [native CI run](https://github.com/rainmana/friendo/actions/runs/40230836566) passed on all three platforms; the separate manual coverage workflow was not dispatched. This is historical base validation, not hosted verification of the local Chat/Expert additions.

## Coverage

[coverage.yml](../.github/workflows/coverage.yml) is manual-only. It uses cargo-llvm-cov 0.8.4 and matching Rust LLVM tools, a separate cache, explicit instrumentation, the full Rust suite, and both external terminal journeys. It exports JSON, LCOV, and readable HTML as workflow artifacts. It does not add a second instrumented build to ordinary PRs, impose an arbitrary percentage gate, or upload private source/context to an external coverage service.

The workflow's measurement step can be reproduced locally:

```sh
set -euo pipefail
cargo llvm-cov clean --workspace
export CARGO_TARGET_DIR="$PWD/target/llvm-cov-target"
coverage_env_file=$(mktemp)
cargo llvm-cov --no-rustc-wrapper show-env --sh > "$coverage_env_file"
source "$coverage_env_file"
: "${RUSTFLAGS:?coverage flags missing}" "${LLVM_PROFILE_FILE:?profile path missing}"
cargo test --all-targets --locked
cargo build --locked --bins
FRIENDO_BIN="$CARGO_TARGET_DIR/debug/friendo" \
FRIENDO_FIXTURE_BIN="$CARGO_TARGET_DIR/debug/friendo-fixture" \
  python3 tests/terminal_smoke.py
```

Export reports from a fresh shell with its normal Cargo target environment:

```sh
mkdir -p target/coverage
cargo llvm-cov report --json --summary-only --output-path target/coverage/summary.json
cargo llvm-cov report --lcov --output-path target/coverage/lcov.info
cargo llvm-cov report --html --output-dir target/coverage
```

The explicit environment file makes setup failure observable before compilation. Instrumentation must reach the externally launched binaries; a successful helper run alone does not prove coverage was collected. When Rust comes from Homebrew rather than rustup, set `LLVM_COV` and `LLVM_PROFDATA` to compatible LLVM tools. This host uses Rust 1.98.1/LLVM 22 and Homebrew LLVM 22 tools. See the [official coverage external-test guidance](https://github.com/taiki-e/cargo-llvm-cov#get-coverage-of-external-tests).

Coverage reports describe executed lines/regions on one platform. They are not branch coverage, assertion quality, exhaustive feature coverage, or proof of safety. Inline unit-test code remains part of source-file totals. The historical released-base baseline used 46 Rust tests without external terminal instrumentation; its follow-up includes 65 tests plus both terminal journeys. The measurements below predate Chat/Expert and do not measure the new presentation paths. Raw local reports live under ignored `target/`; the [compact comparison](verification/coverage-2026-10-25.json) retains counts, scope, and raw-report hashes.

| Source-file line coverage | Before | After |
| --- | --- | --- |
| Application `src/*.rs` (excluding the fixture) | 67.66% | 85.10% |
| Configuration | 70.54% | 98.21% |
| CLI entry point | 0.00% | 74.27% |
| Journal/artifact store | 92.93% | 98.31% |
| TUI | 43.24% | 72.59% |

The after-run uses explicit instrumentation for external binaries. Symbol inspection confirmed LLVM profiling in the instrumented TUI binary, and the completed PTY journeys contributed to its measured coverage. During setup, a shell process-substitution form did not reliably populate this host's instrumentation environment; the checked environment-file form avoids that silent failure. Two existing one-second session readiness waits also failed under full instrumentation and were replaced with observed-output waits bounded at five seconds. The complete instrumented and normal suites then passed.

## Platform limits

The released base's local runtime checks used macOS/aarch64 with Homebrew Rust 1.98.1. Its hosted execution with Rust 1.99.1 passed 65 tests on Linux/x86_64 and macOS/aarch64, and 63 on Windows/x86_64 MSVC. Windows had 19 unit and 17 process tests; Unix had 20 and 18 because of platform-specific cases. Both had 7 configuration, 5 CLI, 10 provider, and 5 runtime integration tests. The nested credential-header subprocess test is not counted twice. Both PTY journeys passed on Linux/macOS; Windows terminal emulation is not covered by that Unix helper. Exact jobs, counts, and log hashes are in the [native verification record](verification/native-ci-2026-10-25.json); current source-build results are separate.

The initial hosted run exposed two Windows assertion mistakes. Rust reports the numeric Windows termination status (1 for this executor), whereas a Unix signal produces no numeric exit code. A child's cwd can also use an 8.3 short spelling of the same directory; the test now canonicalizes observed and expected paths. Native reruns verified both fixes without changing executor behavior. Primary references: [Rust exit status](https://doc.rust-lang.org/std/process/struct.ExitStatus.html#method.code) and [canonicalize](https://doc.rust-lang.org/std/fs/fn.canonicalize.html).

Earlier local Windows GNU cross-checking stopped at ring's missing MinGW compiler. A temporary process/store/model harness passed Windows-target compilation with rustup stable 1.96.0, excluding provider/TLS code. The later native MSVC run exercises the full suite. Windows Job Object pre-assignment races remain explicitly unverified despite observed cleanup in the tested scenarios.

No Protobuf migration or runtime format change is part of this work. Nextest and mutation tooling remain future targeted evaluations, not new per-push costs.
