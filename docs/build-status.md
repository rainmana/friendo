# Build status — 2026-10-25

Runnable local v0.1.0 vertical slice. Native Rust/Ratatui application with a shared controller for TUI, headless model runs, and direct command execution. No model download, Node service, Python service, publication, deployment, or account change is part of operation.

## Implemented

- Offline guided symbol-inventory demonstration, with every fixture view/export identified as synthetic. It reveals invocation/evidence, revises the goal to exported symbols, retains the original constraint and old evidence, opens Rewire, records delivery/interpretation/filter checks separately, and earns a conditional reconnection receipt.
- Streaming OpenAI-compatible Chat Completions and native Anthropic Messages, configured aliases/endpoints/model IDs/ENV credential references/options, fragmented arguments gated until successful completion, native continuation, explicit capabilities, optional usage and unknown pricing.
- Direct argv execution, explicit shell evaluation, persistent native pipe sessions: start/read/write/interrupt/wait/terminate. Session PID/status/artifact references persist across model turns and enter subsequent outbound context.
- Real stdout/stderr spooling and concurrent stdin, bounded excerpts, per-stream disk caps, timeouts, cancellation, Unix process groups, guarded Windows Job Object support, private Unix output files, control-sequence sanitization.
- Schema-1 JSONL journal, UUID run/event IDs, branch ID, sequence, sources/causes, retained output/request/response artifacts, deterministic reducer, read-only review, truncated-tail recovery, unknown outcomes without automatic replay.
- Exact intent and corrections, preserved constraints, interpretation records, explicit acknowledgments, canonical possible-failure reports with basis, pins with observable source inclusion, five canonical Rewire mappings, explicit NORMAL/YOLO.
- Runtime/Coordination/Evidence/Domain graph lenses with sourced entities/edges, stable placement/selection, list alternative, inspection, pan/zoom/focus, grouping, type/relation filters, upstream trace/downstream impact.
- Command composer, exact outbound body inspector, effective config values/sources/precedence/boundaries, remappable repair/YOLO/trace/pin keys with active hints, effects off/reduced/full/overdrive, no-color and narrow layouts.
- Provider-free /tool actions, /ack, /constraints, /report, /conflict, and in-memory /continue. See README for syntax.

## Verification actually executed

Host: **macOS / aarch64**, Rust **1.98.1**, Cargo **1.98.1**. No native Windows/Linux execution or cross-compilation claim.

| Check | Result |
| --- | --- |
| cargo build --all-targets --locked and cargo build --locked --bins | Passed |
| cargo fmt --all --check | Passed |
| cargo clippy --all-targets --locked -- -D warnings | Passed |
| cargo test --all-targets --locked | **65 passed**: 20 unit, 7 configuration, 5 CLI, 18 native process, 10 provider, 5 integrated runtime |
| cargo test --doc --locked | Passed; no doctest examples currently defined |
| demo --headless, doctor, runs, replay --headless --at, replay --context | Executed successfully |
| friendo exec echo -- 'hello from friendo' | Real native invocation, exit 0; stdout/stderr retained; group absence observed |
| Native interactive TUI | Guided reveal/correction/Rewire/repair, YOLO, history, inspectors/help, quit exercised |
| python3 tests/terminal_smoke.py | Passed: event-driven demo plus fixture-backed live mode; resize 140×44 → 60×24 → 140×44, Unicode paste, exact context/tool result, remapped repair/autonomy, correction and Ctrl-C against separate held HTTP streams, read-only history/replay, clean exit and terminal restoration |
| Ratatui backend | 140×44, 60×24, 38×14, 20×8; off/no-color rendering and history controls tested |
| Visual inspection | Normal frame PNG inspected; narrow text capture inspected; label overlap fixed and narrow layout changed to list |
| Live Anthropic model/tool loop | **Passed**: authenticated catalog selection of claude-haiku-5-5; two streaming requests, one real printf command, exact context/result audit; see [live report](live-anthropic.md) |
| Windows/Linux | CI matrix configured, **not executed here** |
| Local coverage | 65 Rust tests plus both instrumented PTY journeys; application source-file line coverage 67.66% → 85.10%, TUI 43.24% → 72.59%; inline unit-test code included; [evidence](verification/coverage-2026-10-25.json) |

The first default-sandbox test attempt could not bind loopback HTTP listeners. Tests were rerun with the necessary local-network permission and passed. This is separate from an application/protocol failure.

The coverage/CI follow-up added policy and credential-reference rejection, journal corruption and failed-write boundaries, artifact containment/size/text checks, partial configuration/provenance/limit/remap tests, and isolated real-binary CLI tests. Replay compares every retained file byte-for-byte and uses both command and HTTP sentinels to detect accidental external actions. The normal preflight is `bash scripts/check.sh`; [testing.md](testing.md) records coverage collection and CI policy. Protobuf and authoritative runtime formats remain unchanged.

The workflows passed local actionlint 1.7.12 validation. Linux preflight gates native macOS/Windows jobs; formatting runs once, platform-specific Clippy remains, superseded revisions cancel, jobs have timeouts, and failures retain logs/terminal recordings. Coverage is manual with a separate cache and includes external PTY journeys. No hosted workflow was dispatched. A full local Windows GNU cross-check stopped at the missing MinGW C compiler required by ring; Windows runtime evidence remains pending. The actual process/store/model modules and process test target were separately compile-checked for Windows in a temporary dependency-isolated harness, without executing them.

Provider fixtures use real loopback HTTP and byte-fragmented SSE. They cover held terminators, incomplete/malformed/truncated calls, native signed continuation, tool ID/result association, auth-header placement, error secrecy, bounded presentation backpressure, and cancellation during stalled headers/body.

Live verification on 2026-10-25 used the existing ANTHROPIC_API_KEY reference and an isolated .friendo/live-verification/anthropic.toml config. Run 47c58f96-42dc-4ece-857a-2e50314e3d3c completed on the first attempt with 56 journal events and no failure event. /usr/bin/printf returned FRIENDO_ANTHROPIC_LIVE_OK followed by a newline (26 stdout bytes), exit 0, empty stderr, and complete capture. Native signed continuation and the exact journaled tool result appeared in the second outbound body. Read-only replay left every run-file hash unchanged; the configured credential value was absent from retained files. The precise output, usage, paths, and audit are in the [live report](live-anthropic.md) and [machine-readable export](verification/anthropic-live-2026-10-25.json). No application fix or protocol-version change was needed.

Integrated fixtures exercise a real model-protocol → native process → follow-up model-protocol loop. Captured HTTP bodies equal retained context bodies, and journaled tool results equal exact model-delivered results. A session survives two separately launched model turns with the same PID/artifact paths; shutdown records observed completion.

Corrections during streaming withhold tools; corrections after a real child is observable cancel that process, retain its result, withhold queued stale actions, and prevent another old-revision model request. Separate tests cover independent evidence, YOLO conflict behavior, untrusted mode-change attempts, zero-timeout rejection, cancellation despite a journal-budget error, escaped/nested credential redaction, large concurrent streams, descendant cleanup, unknown completion, and damaged tails.

Captures: [normal frame](captures/demo.png), [narrow frame](captures/narrow.txt), [native terminal recording](captures/demo.cast). The PNG is a rasterization of the actual Ratatui TestBackend frame, not an OS screenshot. The .cast is generated/ignored; rerun the optional helper to reproduce it. Rasterization used a bundled QA library; the application has no such runtime dependency.

## Limits and interpretation

This is a narrow first usable version. It is not production-readiness evidence.

- Live evidence covers one bounded Anthropic text/tool loop on claude-haiku-5-5, selected from the authenticated model list. Other hosted models, explicit thinking configurations, and live OpenAI-compatible endpoint checks remain unverified. After verification, the workspace default friendo.toml was configured for that model and ANTHROPIC_API_KEY with NORMAL autonomy and the standard execution policy. The isolated live config remains local; the test/CI follow-up did not change the provider default.
- Sessions use pipes, with a configured deadline (default 30s); PTY terminal emulation, a real r2/debugger-specific smoke test, and cross-process session reattachment are not implemented.
- Unix process-group results explicitly leave escaped descendants untracked. Windows Job Object assignment has a pre-assignment race and is unexecuted here. Incomplete termination is not success.
- Executable admission is not an OS sandbox. Generic natural-language constraints remain explicit outbound instructions, not universally enforceable predicates. Provider credential ENV names are removed from child environments and cannot be requested through model ENV references; output redaction protects known literal values, not every possible encoding or exfiltration channel.
- Semantic disagreement is not automatically diagnosed. Revision mismatches/stale explicit dependencies are mechanically observable; possible failure reports carry provenance. Live replies produce new current nodes; old historical plans/answers stay stale until explicitly re-evaluated.
- LINK RESTORED is earned only by the guided fixture's documented delivery, interpretation, exported-only filter, and retained-original-source checks. The fixture accepts its displayed correction only; arbitrary corrections require live operation. This says nothing about identical human/model understanding.
- Rewire Prioritize/Split express ordering/meaning in editable correction text. Dedicated numeric weights, alternate-interpretation editors, general workflow/context-rule editors, and a full keymap editor remain deferred.
- Missing usage, price, tool versions, and provider-internal context remain unknown. No comprehensive tool-version probing or performance/RSS benchmark is claimed.
- Retention is local and indefinite until explicitly removed. Known credential values are redacted; arbitrary sensitive source text is not classified automatically. Lossy decoding, bounded excerpts, redaction, and truncation limit replay fidelity.
- Journals retain at most 100,000 events / 64MiB serialized event content, with a 1MiB per-event cap. Artifacts have separate limits. UI folds bounded event prefixes and has not been profiled at those limits.
- Review is implemented. Disk resume/rerun/fork, snapshot/rollback, process migration, distributed swarms, marketplace, discovery, and exactly-once external execution are unavailable.

## Next bounded milestone

Run the native suite on Windows/Linux CI and repair any platform-specific failures. Then add an isolated PTY component and verify one persistent r2/debugger workflow without expanding execution authority. The first bounded live Anthropic verification is complete.

Canonical MCW and Graffy revisions, implementation assessment, and primary references are recorded in [sources.md](sources.md) and [graffy-assessment.md](graffy-assessment.md).
