# Graffy reuse assessment

Inspected 2026-10-25 at [`7d79fc2e5b816fc5793ff988ddbf34fba800b782`](https://github.com/rainmana/graffy/tree/7d79fc2e5b816fc5793ff988ddbf34fba800b782), source-tree version `0.1.0-alpha.6`. A shallow checkout was inspected through the CodeRLM tree-sitter index, including source bodies and test assertions. The README was context, not evidence of production readiness.

**Decision:** retain Graffy as a design reference; do not import a crate or copy source for the first friendo slice. This is a scope and integration-cost decision. Graffy contains real execution and provenance work; it is not merely a README or empty scaffold.

## Concrete findings

All paths and line numbers below refer to the pinned upstream revision.

| Area | Source and assertions inspected | Fit for friendo |
| --- | --- | --- |
| Journal | `crates/graffy-core/src/journal.rs:42–65,128–144` exclusively creates journals, assigns sequence numbers, writes then flushes protobuf frames, and mirrors them to a tap. `frames_roundtrip_in_order` checks ordered round trip and reducer status; `tap_mirrors_exactly_what_was_written` checks tap equality. | Useful invariants. Reuse would import Graffy protobuf types and its executor timestamp helper. The tap is an `UnboundedSender` at line 29. friendo needs bounded presentation independently of authoritative persistence. |
| Recovery | `journal.rs:184–204` reads the entire file, decodes every frame, and rejects broken sequence/run identity. `crates/graffy-core/tests/release_regressions.rs:358–387` verifies existing bytes survive attempted creation and inconsistent identity/sequence is rejected. | Good overwrite protection. A decode failure returns an error without a valid-prefix result. friendo's damaged-tail recovery and bounded JSONL reader require different behavior. |
| Graph | `crates/graffy-core/src/graph.rs:44–131` compiles node/guard execution specs and rejects unguarded cycles, duplicate IDs, and unknown endpoints. Tests at `232–285` assert unguarded-cycle and unknown-target errors; `288–326` checks successor order and guards. | An executable graph compiler is more machinery than the initial controller needs. Its edges schedule work. friendo's evidence and visual edges must remain projections without execution authority. |
| Coordination records | `src/protos/mcw.proto` distinguishes actors, operational claim-support levels, empirical MCW evidence layers, failure signals, repairs, and provenance. `crates/graffy-mcw/src/lib.rs:12–32` only re-exports wire types and lists five repair node names; its test checks count and prefix. | The schema is informative. Importing the MCW crate would not supply friendo's revision invalidation, editable Rewire, delivery transitions, or checks. Avoid claiming the crate's comments establish live detectors. |
| Execution | `crates/graffy-core/src/exec.rs:254–289` exposes whole-completion `ModelInvoker` requests/results. `control.rs:39–55` races cancellation and deadline against a future. `exec.rs:2717–2824` tests retry feedback as a correction IU, same-session linkage, repair back-link, and unknown target-failure resolution despite run success. | Strong attribution pattern worth retaining conceptually. It does not directly provide friendo's streaming tool calls, generic direct CLI execution, process-descendant supervision, and persistent pipe sessions. Adopting the graph executor would broaden the product into a separate scheduling architecture. |
| Cancellation | `release_regressions.rs:136–166` uses a stalled fake model, triggers cancellation, bounds the wait, and asserts `Cancelled` with no final text. | Meaningful model-wait cancellation test. It does not establish OS descendant termination or PTY behavior. Those need friendo-specific native tests. |

One precise upstream assertion is particularly relevant to honest repair feedback:

```rust
assert_eq!(
    repair.target_failure_resolved, None,
    "run success cannot establish resolution of the same misalignment"
);
```

Source: [`exec.rs:2807–2810`](https://github.com/rainmana/graffy/blob/7d79fc2e5b816fc5793ff988ddbf34fba800b782/crates/graffy-core/src/exec.rs#L2807). Quoted for assessment; this is not vendored implementation.

## Verification boundary

Attempted in the separate temporary checkout:

```console
cargo test -p graffy-core -p graffy-mcw --offline
```

It stopped during dependency resolution: `no matching package named rigs found`, required by the workspace's `graffy-agents`, despite selecting the smaller packages. No Graffy test was executed successfully in this assessment, and no model was contacted. The findings above describe inspected source and assertions, not reproduced upstream test results. A whole Graffy dependency download/build was not necessary to make the narrow reuse decision.

## Attribution and license inventory

Upstream `Cargo.toml` declares **GPL-3.0-or-later**, `LICENSE` contains GPL v3, and `NOTICE.md` credits the Apache-2.0 MCW framework, MIT Rig, and other lineage. friendo already has GPL v3 license text. No Graffy crate or implementation source is included in this milestone; this assessment preserves the upstream identity and revision. Any later source reuse must retain applicable copyright, license, and notice material and be reviewed against the actual files imported. Framework terminology is attributed directly in [sources.md](sources.md).
