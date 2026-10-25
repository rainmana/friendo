# Local verification and CI

The required preflight is:

```sh
bash scripts/check.sh
```

It runs `cargo fmt --all --check`, `cargo clippy --all-targets --locked -- -D warnings`, `cargo test --all-targets --locked`, `cargo test --doc --locked`, `cargo build --locked --bins`, and the Unix terminal helper on Linux/macOS. Cargo compiles what each command needs; a build or `cargo test --no-run` alone does not execute assertions. There are no application-defined Cargo features to multiply into a feature powerset matrix.

The current macOS/aarch64 suite has 65 passing Rust tests: 20 library unit, 7 configuration, 5 real CLI, 18 native process, 10 provider, and 5 runtime integration tests. Doctests are checked, with zero examples currently defined. Provider and replay-sentinel tests need local loopback networking. No hosted credential is needed for preflight.

## Covered boundaries

- Policy: provider credential names cannot be either the destination or source of an environment override; ordinary/missing references, deadline rejection/clamping, malformed arguments, shell-disabled dispatch, and unsupported tool fields are exercised. Rejected calls must not append `ToolStarted`.
- Storage: complete corruption, invalid UTF-8, unknown event type/schema, broken sequence, mixed run identity, oversized records, and failed actual writes fail without applying the proposed transition. Artifact tests cover canonical path containment, sibling-prefix escape, missing/oversized/non-text files, and Unix symlink escape.
- Configuration: partial defaults, provenance for explicitly supplied default values, unknown fields, numeric boundaries, remap collisions/reserved keys, exact executable paths, explicit wildcard admission, and YOLO's unchanged admission policy.
- CLI: real subprocesses use explicit temporary config/data and bounded waits. Tests exercise `doctor`, defaults, precedence, credential readiness/secrecy, invalid input, synthetic demo, listing, replay prefixes, and exact context. Replay compares all retained file bytes and checks HTTP/command sentinels, including an uncertain action and damaged tail.
- Processes: real fixture processes, descendants, cancellation, output bounds, sessions, redaction, and deadlines remain covered. Deadline observation uses completion waits rather than a fixed post-deadline sleep. Windows assertions check empty Job Object evidence and descendant exit; a Windows-only test checks unsupported pipe interruption followed by observed termination. Drop/orphan cleanup checks also compile for Windows.
- Terminal: the standard-library Python helper runs the real Rust executable in a Unix PTY. It waits for durable events and visible cell states, rather than scheduling keys at fixed elapsed times. Its small screen observer handles the cursor/erase/style subset emitted by Ratatui; it is QA code, not product process-terminal emulation.

The terminal helper runs two isolated journeys. The synthetic demo checks repair, NORMAL/YOLO, resize, and historical controls. Live mode uses an explicit loopback fixture, one real harmless fixture command, exact journal-to-model tool results, exact retained HTTP bodies, Unicode bracketed paste, context inspection, remapped controls, constraint retention, correction against a held third response, then Ctrl-C against a separately held fourth response. The two canceled streams must never become final answers. Real replay verifies the final reducer state. Both journeys verify exit zero and terminal-mode/cursor/alternate-screen/bracketed-paste restoration.

Generated captures are `docs/captures/demo.cast` and `docs/captures/live-fixture.cast`, ignored by Git. `FRIENDO_BIN` and `FRIENDO_FIXTURE_BIN` can select instrumented binaries. Temporary runs/configs never consult personal `friendo.toml` or existing runs. This is fixture-backed live-mode evidence, separate from [the real Anthropic verification](live-anthropic.md).

## CI policy

[ci.yml](../.github/workflows/ci.yml) runs for PRs, pushes to `main`, and manual dispatch. Feature-branch pushes without a PR do not consume a second hosted run. Workflow concurrency cancels obsolete runs for the same PR/branch. No path filter can strand a required status on a docs-only PR.

Linux runs the full preflight first. macOS and Windows run after that gate, retaining native Clippy for platform-specific `cfg` code and native tests on all three operating systems. Unix runners also execute the PTY helper. Tests are not retried automatically. Jobs have bounded timeouts; failures upload logs and credential-free terminal captures for seven days. Existing Rust caching remains.

The real CLI integration tests replace standalone headless-demo/doctor exit-zero smoke commands. Native tests still matter: compiling Windows code cannot prove Job Object behavior, and fixture HTTP cannot prove hosted model support. No CI run has been pushed or dispatched as part of this local implementation.

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

Coverage reports describe executed lines/regions on one platform. They are not branch coverage, assertion quality, exhaustive feature coverage, or proof of safety. Inline unit-test code remains part of source-file totals. The baseline used 46 Rust tests without external terminal instrumentation; the follow-up includes 65 tests plus both terminal journeys. Raw local reports live under ignored `target/`; the [compact comparison](verification/coverage-2026-10-25.json) retains counts, scope, and raw-report hashes.

| Source-file line coverage | Before | After |
| --- | --- | --- |
| Application `src/*.rs` (excluding the fixture) | 67.66% | 85.10% |
| Configuration | 70.54% | 98.21% |
| CLI entry point | 0.00% | 74.27% |
| Journal/artifact store | 92.93% | 98.31% |
| TUI | 43.24% | 72.59% |

The after-run uses explicit instrumentation for external binaries. Symbol inspection confirmed LLVM profiling in the instrumented TUI binary, and the completed PTY journeys contributed to its measured coverage. During setup, a shell process-substitution form did not reliably populate this host's instrumentation environment; the checked environment-file form avoids that silent failure. Two existing one-second session readiness waits also failed under full instrumentation and were replaced with observed-output waits bounded at five seconds. The complete instrumented and normal suites then passed.

## Platform limits

All runtime results in this change were executed on macOS/aarch64. Linux and Windows native runtime tests await hosted execution. A local full Windows GNU cross-check could not pass ring's C build because MinGW is absent. A temporary harness using the actual process/store/model sources, fixture executable, and process tests did pass Windows-target compilation with rustup stable 1.96.0; it excluded the provider/TLS layer and did not execute Windows binaries. The main local build/test compiler is Homebrew Rust 1.98.1. Windows Job Object pre-assignment races remain explicitly unverified in runtime results.

No Protobuf migration or runtime format change is part of this work. Nextest and mutation tooling remain future targeted evaluations, not new per-push costs.
