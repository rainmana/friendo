# Test coverage and CI efficiency research

Date: 2026-10-25. Research by a separate dedicated subagent. No tests, runtime code, dependencies, or workflows were changed; no tools were installed and no hosted CI or live inference was run for this assessment.

## Findings

The existing tests assert substantial behavior. There is no measured coverage report, so neither a percentage nor maximum coverage is established. Close concrete gaps and improve check ordering before pursuing a global 100% target.

| Layer | Current evidence | Limit |
| --- | --- | --- |
| Unit | 13 tests: authority/staleness, repair, cancellation despite journal failure, credentials, recovery, TUI | No dedicated configuration tests; uncovered rejection branches |
| Native process | 18 tests: literal argv, concurrent streams/stdin, caps, cancellation, descendants, sessions, redaction, private files | Unix assertions do not establish Windows supervision |
| Provider | 10 tests using real loopback HTTP: fragmented SSE, completed-call gating, native signed continuation, auth, cancellation, bounds | Fixtures do not prove hosted model compatibility |
| Runtime | 5 tests connecting HTTP, real fixture processes, exact context/journal assertions, corrections, sessions | Integrated fixture uses OpenAI-compatible responses; Anthropic has adapter fixtures and separate live evidence |
| TUI components | Historical controls cannot dispatch; stable selection; rendering at four sizes | Most composer commands, remaps, graph interactions, live interruption are not asserted |
| Terminal journey | Existing Unix PTY helper covers demo repair, resizing, exit status, control restoration and termios | Absent from CI; synthetic demo only; timed key delivery |
| Live service | One recorded Anthropic model/tool/model loop with context, replay, and credential audit | One bounded macOS run is not a recurring platform regression suite |

These counts were subsequently rechecked by the parent: all 46 tests passed locally on macOS/aarch64 with the new default config. See [build status](../build-status.md), [process tests](../../tests/processes.rs), [provider tests](../../tests/providers.rs), [runtime tests](../../tests/runtime.rs), [terminal helper](../../tests/terminal_smoke.py), and [live evidence](../live-anthropic.md).

A strong assertion to preserve is [tests/runtime.rs:341](../../tests/runtime.rs#L341), which compares the retained inspector artifact to the actual HTTP body:

```rust
assert_eq!(
    &retained, actual,
    "context inspector must show actual outbound content"
);
```

## Prioritized gaps

1. **Policy and credential references.** The test named `model_cannot_remove_deadline_or_change_provider_environment` at [src/controller.rs:1200](../../src/controller.rs#L1200) currently asserts timeout rejection/shortening, without testing provider-environment rejection. Add destination-name and reference-name collisions with configured credentials, allowed ordinary references, missing references, shell-disabled dispatch, exact absolute-path admission, malformed args, and unsupported tool fields. The real boundaries are [src/controller.rs:779](../../src/controller.rs#L779) and [src/config.rs:131](../../src/config.rs#L131). These cases can remain fast and offline.
2. **Journal and artifact rejection.** Existing store tests cover redaction, unique creation, and truncated final tails. Add complete malformed records, unsupported schema, sequence gaps, mixed run IDs, oversized records, artifact escape/symlink/missing/oversized cases, and a replay CLI test proving files remain unchanged and no external action occurs. Relevant branches: [src/store.rs:165](../../src/store.rs#L165) and [src/store.rs:212](../../src/store.rs#L212).
3. **Configuration and CLI.** Test partial defaults, unknown fields, numeric boundaries, remap collisions/reserved keys, explicitly supplied values equal to defaults, and CLI precedence/source reporting. Exercise real `doctor`, `runs`, `replay`, invalid config, and headless-without-goal. Relevant source: [src/config.rs:66](../../src/config.rs#L66), [src/config.rs:89](../../src/config.rs#L89), [src/main.rs:89](../../src/main.rs#L89). `doctor` exits normally after printing credential readiness; exit zero alone does not prove availability or a live request.
4. **Native platform behavior.** Execute Linux and Windows tests before portability claims. Windows Job Object assignment/cleanup needs native assertions ([src/process.rs:751](../../src/process.rs#L751)); macOS cannot exercise it. Unix-only descendant/interrupt assertions appear at [tests/processes.rs:133](../../tests/processes.rs#L133) and [tests/processes.rs:327](../../tests/processes.rs#L327). Windows pipe-session interruption is unavailable by design and should be asserted, followed by successful termination ([src/process.rs:745](../../src/process.rs#L745)).
5. **Automated terminal journeys.** Run the existing PTY helper in Unix CI. Then test live mode against a local fixture: compose, streamed reply, tool, inspect actual context, correction, Ctrl-C, quit. Include paste/Unicode, remaps, NORMAL/YOLO constraints, historical review, and restoration on errors. Windows needs a separate terminal strategy: the current helper imports Unix-only modules ([tests/terminal_smoke.py:3](../../tests/terminal_smoke.py#L3)).
6. **Deterministic synchronization.** Replace fixed-time PTY key delivery with observed markers and an overall deadline before scaling concurrency ([tests/terminal_smoke.py:27](../../tests/terminal_smoke.py#L27)). Investigate short process deadlines under real CI load before adding blanket delays. Existing runtime tests demonstrate one-shot gates and observed child state ([tests/runtime.rs:444](../../tests/runtime.rs#L444)).

## Compilation and local preflight

`cargo build` compiles production targets; it does not execute test assertions. `cargo test --no-run` compiles tests without executing them. `cargo test --all-targets` executes selected target tests, but does not mean every Cargo feature combination or doctests. [Official Cargo test documentation](https://doc.rust-lang.org/cargo/commands/cargo-test.html).

Proposed local preflight, retaining repository-required checks:

```sh
cargo fmt --all --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --all-targets --locked
cargo build --locked --bins
python3 tests/terminal_smoke.py
```

The PTY step is Unix-only. Use explicit fixture configuration and temporary data directories for added CLI smoke tests, avoiding dependence on personal `friendo.toml` and retained runs. Do not add an unconditional `cargo test --no-run` or `cargo build --all-targets` immediately before the same full test run: Cargo already compiles what it needs.

There are currently no project-defined Cargo features, so a feature powerset matrix adds no application-feature coverage.

## GitHub Actions efficiency

The existing three-OS workflow repeats formatting, Clippy, tests, headless demo, and doctor ([.github/workflows/ci.yml:20](../../.github/workflows/ci.yml#L20)):

```yaml
- run: cargo fmt --all --check
- run: cargo clippy --all-targets --locked -- -D warnings
- run: cargo test --all-targets --locked
- run: cargo run --locked --bin friendo -- demo --headless --effects off
- run: cargo run --locked --bin friendo -- doctor
```

Recommended design, not implemented:

- Use an inexpensive Linux formatting/common-lint gate before native matrix tests. Preserve platform-specific Clippy where it covers distinct `cfg` code.
- Keep Linux/macOS/Windows native tests: process supervision makes these valuable. Cancel superseded revisions rather than dropping essential platform coverage.
- Avoid duplicate branch-push plus PR runs where appropriate. Add concurrency keyed to workflow and PR/branch, canceling obsolete work. [Official GitHub concurrency documentation](https://docs.github.com/en/actions/how-tos/write-workflows/choose-when-workflows-run/control-workflow-concurrency).
- Add job timeouts and useful failure artifacts. Avoid blanket retries that conceal races.
- Keep the existing `Swatinem/rust-cache`; dependency/compiler/host-aware caching is already present. Keep coverage-instrumented caches separate. Caching does not replace tests or eliminate all workspace compilation. [Official rust-cache documentation](https://github.com/Swatinem/rust-cache).
- Evaluate docs-only filtering against branch protection first; required statuses must still report.
- Keep live model checks manual/explicit with request and token bounds. Ordinary PR CI should be deterministic and credential-free.
- No billing/runtime measurements were collected, so savings percentages are unknown.

## Coverage tools to evaluate

**cargo-llvm-cov first:** establish a line/region baseline, then inspect authority, cancellation, parsing, and recovery branches before setting a threshold. Its external-test workflow can include a CLI driven by the PTY helper. Branch coverage requires optional nightly support; stable line/region coverage is a useful starting point. [Official documentation](https://github.com/taiki-e/cargo-llvm-cov).

**cargo-nextest after timing:** isolation, timeouts, reporting, and scheduling may help, but recheck the nested auth subprocess and process-heavy tests before migration. No speedup was measured. Doctests still need `cargo test --doc`. [Official nextest documentation](https://nexte.st/), [runner design](https://nexte.st/docs/design/how-it-works/).

**cargo-mutants selectively:** audit assertion quality in configuration admission, reducer authority/revision logic, and recovery. Run bounded subsets manually or periodically, with timeouts, instead of multiplying full mutation runs across every platform and push. [Official results guidance](https://mutants.rs/using-results.html), [timeouts](https://mutants.rs/timeouts.html).

Measure the existing suite, close priorities 1–4, and add missing Unix terminal CI coverage. These steps improve confidence and avoid hosted failures more directly than a global coverage percentage target. This report proposes that work; it does not implement it.
