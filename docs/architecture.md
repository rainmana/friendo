# Architecture and decisions

friendo is one Rust crate with a shared library, a CLI/TUI binary, and a small native fixture executable. The first slice owns its controller and coordination transitions. It uses `reqwest` for two provider protocols and Ratatui/Crossterm for the terminal. The custom cell effects are presentation over recorded state; no effect can dispatch tools or append a successful check.

## Boundaries

| Module | Responsibility | Authority boundary |
| --- | --- | --- |
| `model.rs` | Versioned events, reducer, nodes/edges, dependency invalidation | The reducer applies recorded observations; graph edges do not authorize execution. |
| `store.rs` | Exclusive run creation, JSONL append, artifacts, redaction, recovery | Persist and sync authoritative events before updating the in-memory reducer. |
| `controller.rs` | Model/tool turn loop, revisions, corrections, pins, policy admission, repair | Recheck current revision/cancellation before dispatch. Only direct user/config actions change autonomy. |
| `provider.rs` | Request encoding, SSE parsing, complete tool calls, native continuation, capabilities | Model content is untrusted input. Validate completed arguments before controller dispatch. |
| `process.rs` | Direct/shell invocation, concurrent output drains, persistent pipes, supervision | Operates with host permissions; reports requested versus observed termination. |
| `tui.rs` | Stable node selection, lenses, inspectors, composer, Rewire, effects | Review controls cannot dispatch; standalone replay has no controller. |
| `main.rs` | `demo`, `run`, `runs`, `replay`, `doctor`, `exec`, headless/snapshot paths | Headless paths share the controller; replay only reads records. |

## Authoritative history and disposable presentation

`Event` uses schema version 1, UUID event/run IDs, branch ID `main`, per-run sequence, UTC timestamp, attributed source, causal event references, and a tagged payload. Branch identity is a seam; branching is not implemented. The source revision of the canonical MCW documents is recorded at run start.

The actual envelope includes:

```rust
pub struct Event {
    pub schema: u32, pub id: String, pub run_id: String, pub branch_id: String,
    pub seq: u64, pub timestamp: String, pub source: String, pub causes: Vec<String>, pub kind: Kind,
}
```

Typed payloads distinguish exact intent/corrections, explicit constraint replacement, model interpretations, context delivery, tool observations, artifacts, checks, conflicts/assumptions, human acknowledgment, and repair receipts. Canonical failure assessments carry mode, basis (mechanical check, model assessment, or human report), possible status, and revision. `/report` records possible human reports; `/ack` records direct acknowledgment. Claims/plans use typed node-kind labels; there is no general claim-entailment engine. Rendering a panel is never acknowledgment.

Every append writes a JSON line and syncs it before applying the reducer. Existing journals are not reused for a new run. A run retains at most 100,000 events and 64 MiB of serialized event content, with a 1 MiB record limit; large content belongs in referenced artifacts. This is a serialized-content budget, not a measured heap ceiling. The current UI retains/folds the event collection rather than maintaining an optimized incremental timeline index. Its scale and frame cost have not been profiled.

Streamed preview uses a bounded 32-message channel and an 8 KiB rolling presentation buffer. Preview updates can be dropped/coalesced by the transport; a complete response artifact is authoritative only after successful stream completion. Error/cancellation leaves an incomplete record instead of a fabricated final answer. Rendering backpressure does not grant tool dispatch or success.

## Model and process loop

At each turn the controller builds the exact outbound body from the configured system prompt, current intent revision, retained constraints, selected pins, previous interpretation, bounded answer history, recorded session IDs/status, and that turn's native tool messages. It retains this body after credential redaction before sending it. Context view renders the retained body and Enter loads it inline. Adapters preserve tool-call IDs and native continuation data; Anthropic signed/redacted thinking blocks remain provider-native continuation, not a claim of visible provider-internal context.

OpenAI-compatible Chat Completions and Anthropic Messages use separate encoders/decoders. Complete tool arguments must parse and pass validation after a successful stream terminator. The provider boundary limits request/response bytes, event size, tool calls, total duration, and idle duration. Capabilities describe this adapter subset separately from the unprobed model/server capabilities. Usage is optional and pricing unknown. [sources.md](sources.md) records why direct transport was selected after considering genai and Rig.

`exec` uses an executable plus argument vector, working directory, stdin, referenced environment overrides, deadline, and per-stream retention cap. `shell` requires explicit shell selection plus separate policy admission. `/tool {id,name,arguments}` submits the same validated dispatch directly from the composer without a provider. The admission list compares exact executable strings (`*` is an explicit broad opt-in); it is not an OS sandbox. Child processes inherit the parent environment after configured provider credential names are removed; overrides cannot reference or target those credential names. No generic semantic checker can enforce arbitrary natural-language constraints.

stdout and stderr drain concurrently into independently capped spool files and 16 KiB excerpt tails. Excess output is drained and counted, with truncation recorded. Known credential values are redacted before spooling; UI/model excerpts also remove control sequences. Incomplete pipe capture is visible in results. There is no terminal-sequence interpreter.

Persistent sessions retain supervised native pipe processes across model turns and expose start/read/write/interrupt/wait/terminate. Initial snapshots/output artifacts are registered at start; `State.sessions` retains IDs/status for the next model turn. Shutdown records observed completion results. The initial timeout still applies. Ctrl-C cancels the active turn and interrupts retained sessions; targeted session operations remain available. Unix process groups and Windows job objects provide platform-specific descendant supervision. Native runtime evidence currently covers macOS; CI files do not establish Windows/Linux runtime behavior. Escaped/detached descendants and incomplete cleanup must not be presented as proven termination.

## Corrections and repair

A correction appends a new intent revision and preserves earlier evidence. `/constraints` explicitly replaces the user constraint array and creates a revision; ordinary corrections retain existing constraints. Revision-tagged older work and known transitive dependencies become stale; independent revision-free evidence remains available. Dependency basis is retained as declared, mechanically derived, model-proposed, or fixture provenance where supplied. Detection is limited to these records, not exhaustive semantic impact analysis.

The controller rechecks revision and cancellation before dispatch. An active dependent turn receives cancellation after the correction; already-started external effects are not undone. The current controller schedules one model/tool loop rather than concurrent independent branches.

Rewire exposes exact/current intent, interpretation, selected provenance, and affected work. Each operation records its canonical operation name with editable correction text in live runs. Expand additionally reopens up to eight retained artifacts as marked 16 KiB pins and exposes source gaps. Prioritize and Split currently express priorities/meaning through correction text; dedicated weight and alternative-interpretation editors are deferred. Artifact pins read actual source-file contents; other pins include the selected label/detail. Missing/oversized sources fail rather than pinning a path. Pins over 64 KiB are rejected, and the inspector records their inclusion policy.

Correction, outbound context, delivery, interpretation update, dependency refresh, explicit acknowledgment, and declared checks are distinct events. The guided demo accepts its specific exported-only correction, filters retained synthetic symbol lines, and checks the original-source condition; arbitrary edited corrections are not supported by that fixture. Only that bounded fixture produces `LINK RESTORED` after its documented conditions pass. Live repair records revision delivery/interpretation checks without inferring semantic agreement. Older answers and artifacts remain reviewable.

NORMAL pauses dependent dispatch for an explicitly reported unresolved interpretation conflict. YOLO records a bounded continuation assumption and continues under the same executable/shell policy. Only user or user-config event attribution can change autonomy; tools, model output, and imported source cannot enable it. No automatic semantic conflict detector is claimed.

## Topology and presentation

Runtime, Coordination, Evidence, and Domain lenses project the same recorded session. Nodes/edges have stable IDs, kind/relation labels, source event IDs, and provenance basis. Layout positions derive from stable arrival order; selection uses IDs. Evidence and execution relationships remain separate from scheduling authority. The Domain fixture contains an explicitly synthetic host/service/observed-link; no network discovery runs.

Pan/zoom, selected/latest focus, upstream/downstream tracing, filters, collapsing, list view, and inspectors operate over the selected event prefix. Historical review folds that prefix, so its graph matches the recorded position. Windows narrower than 90 columns use the list view, and footer/help expose active remaps. Effects are short, local, configurable, and driven by events; off/reduced/no-color paths preserve controls. The terminal guard and panic hook restore raw/alternate-screen state. The target frame budget is configurable, without a performance claim.

## Retention, configuration, and recovery

Configuration precedence is CLI > TOML > defaults. The inspector tracks explicitly supplied top-level file fields, including values identical to defaults, and exposes values, source, precedence, and effective boundary. File changes apply next run; direct autonomy changes apply next dispatch. Supported customization covers aliases/endpoints/model IDs/provider options, system prompt, execution limits/admission, four key remaps, map/list layout, FPS, color, and effects. General workflow/context-rule editors are deferred.

Credential environment references are retained, not their configured values. Known provider credential values and explicit environment override values are redacted before process spool retention; journals/artifacts redact configured provider credentials, including decoded strings inside escaped/nested JSON. Configured provider credential names are removed from child environments, and tool overrides cannot pass them through. Arbitrary confidential text remains local retained content. Source/output transformations, lossy decoding, bounded excerpts, and truncation limit replay fidelity. There is no expiry/pruning automation. Unix run directories/files use private permissions; no equivalent cross-platform ACL guarantee is claimed.

Recovery reads valid newline-terminated records before an incomplete final tail, reports the tail, and leaves the file unchanged. Complete malformed records, unsupported schema, broken sequence, or mixed run identity fail explicitly. Started actions and retained sessions with no reliable completion retain **outcome unknown**. Recovery/review never silently replay or reattach an external side effect.

`/continue` operates within an active application's current environment and in-memory context. Review is implemented. Disk resume, rerun, fork, external-state snapshot/rollback, exactly-once side effects, PTY emulation, and process migration are unavailable.

## MCW scope

MCW is emergent human–AI coordination. The journal records observations that can support reflection and study; it does not store/implement MCW, reveal HCW contents, or establish a universal alignment percentage. Mechanical revision checks, model statements, and human reports have different bases. Canonical taxonomy and application mappings are attributed in [sources.md](sources.md). No measured MCW-improvement or validated research-instrument claim is made. Graffy's concrete implementation and test assertions were assessed in [graffy-assessment.md](graffy-assessment.md); no Graffy implementation was imported.
