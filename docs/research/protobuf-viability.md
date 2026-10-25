# Protocol Buffers viability research

Date: 2026-10-25. Research by a dedicated subagent; no codec, schema, dependency, runtime, or CI changes were implemented. No benchmark was run.

## Recommendation

Keep the current JSONL journal, provider JSON/SSE, TOML configuration, and raw output artifacts. Protocol Buffers are most promising for a future external executor/daemon or cross-language event interface. Current in-process communication already moves Rust types through Tokio channels, so adding serialization there would add work.

| Boundary | Assessment | Next decision |
| --- | --- | --- |
| Future executor/daemon/remote IPC | Strongest potential fit for generated clients, compact envelopes, and typed contracts | Revisit when this interface exists; retain authorization and revision checks |
| Future domain imports/event subscriptions | Plausible for high-volume observations and independent clients | Define transport DTOs separately from authoritative domain types |
| Disposable replay cache/index | Plausible after profiling | Safer experiment than changing authoritative storage |
| Authoritative journal | Feasible but costly to migrate safely | Require measured benefit and an explicit compatibility decision |
| Provider transport | Poor fit: the existing services require JSON/SSE | Preserve native provider content without lossy conversion |
| Configuration, inspectors, command output | Poor fit | Preserve editable and inspectable formats |

## Existing contracts

JSON does not mean untyped. [src/model.rs:25](../../src/model.rs#L25) defines the event envelope, followed by an internally tagged payload enum:

```rust
pub struct Event {
    pub schema: u32,
    pub id: String,
    pub run_id: String,
    pub branch_id: String,
    pub seq: u64,
    pub timestamp: String,
    pub source: String,
    pub causes: Vec<String>,
    pub kind: Kind,
}
```

[src/store.rs:113](../../src/store.rs#L113) sanitizes the event through a JSON tree and makes it durable before applying it:

```rust
let mut value = serde_json::to_value(&event)?;
self.redact_value(&mut value);
let text = serde_json::to_string(&value)?;
```

```rust
let event: Event = serde_json::from_str(&text)?;
self.file.write_all(text.as_bytes())?;
self.file.write_all(b"\n")?;
self.file.sync_data()?;
self.state.apply(&event);
```

A binary codec might reduce repeated field names and parsing/allocation, but this path also pays for redaction, synchronous durability, reducer work, and retained objects. End-to-end gains have not been measured. The existing bounds are 100,000 events, 1 MiB per event, and 64 MiB of serialized events ([src/store.rs:98](../../src/store.rs#L98)).

Provider continuation is deliberately extensible ([src/provider.rs:193](../../src/provider.rs#L193)):

```rust
pub struct ProviderReply {
    pub text: String,
    pub tool_calls: Vec<ToolCall>,
    pub continuation: Value,
    pub usage: Option<Value>,
}
```

The Anthropic adapter preserves native content arrays, including signed content, at [src/provider.rs:355](../../src/provider.rs#L355); transport sends JSON at [src/provider.rs:442](../../src/provider.rs#L442). Mapping these opaque fields into a closed protobuf schema adds fidelity risk. Wrapping original JSON bytes preserves fidelity but reduces the proposed benefit.

## Requirements before a future experiment

- **Framing and recovery:** protobuf messages are not self-delimiting. Supply bounded framing, a format identifier, and a corruption policy. Preserve current recovery behavior: only an incomplete final record is recoverable; a complete invalid record fails. Test partial prefixes/payloads, complete corruption, sequence/run/schema validation, and uncertain external actions. Current recovery is at [src/store.rs:212](../../src/store.rs#L212). [Official streaming guidance](https://protobuf.dev/programming-guides/techniques/).
- **Evolution and authority:** use stable field numbers and reserve removed numbers/names. Validate unknown enums and missing event payloads. Wire compatibility alone does not justify silently skipping an unknown authority-changing event during replay. [Official evolution rules](https://protobuf.dev/programming-guides/proto3/).
- **Unknown fields:** ordinary Prost generated decoders skip unknown fields; unknown enum preservation does not imply preservation of all future fields through parse/re-encode. Retain original bytes or explicitly test a representation that satisfies the required behavior. [Prost documentation](https://docs.rs/prost/latest/prost/), [versioned derive source](https://github.com/tokio-rs/prost/blob/v0.14.4/prost-derive/src/lib.rs#L174-L176).
- **Presence:** `ProcessResult.exit_code: Option<i32>` at [src/process.rs:50](../../src/process.rs#L50) must distinguish absence from successful exit zero. Usage also distinguishes missing from reported zero. Use explicit presence and validate application requirements. [Official presence guidance](https://protobuf.dev/programming-guides/field_presence/).
- **Redaction:** replace JSON-tree traversal with explicit typed redaction, including embedded JSON and opaque bytes. Preserve non-serialization of invocation redaction material at [src/process.rs:40](../../src/process.rs#L40). Binary encoding does not protect secrets.
- **Inspection:** provide readable exports and retain exact outbound provider bodies. ProtoJSON has different mappings and evolution properties from serde JSON. [Official ProtoJSON format](https://protobuf.dev/programming-guides/json/).
- **Integrity:** protobuf serialization is not canonical; hash retained original bytes rather than assuming re-encoding produces an invariant semantic fingerprint. [Official serialization guidance](https://protobuf.dev/programming-guides/serialization-not-canonical/).

## Measurement plan

Use synthetic or redacted small, typical, and near-limit corpora. Include metadata-heavy and text-heavy runs. Measure encoding/decoding separately from redaction, append plus `sync_data`, recovery plus reducer, peak memory, and retained bytes. Compare JSONL, compressed archival JSONL, and bounded protobuf framing without changing durability or retention semantics between candidates. Fault-test every framing truncation boundary, incompatible schemas, unknown payloads, absent-versus-zero fields, and credential redaction.

No performance claim follows from this research. Profile journal/replay/UI behavior before choosing an optimization.

## Rust and build implications

Prost is a plausible native Rust candidate. Its standard build path adds `protoc` and schema/codegen maintenance; checked-in generated code avoids a compiler requirement for normal builds but needs pinned regeneration and drift checks. Do not add gRPC/tonic without an actual service transport requirement. [Prost build documentation](https://docs.rs/prost/latest/prost/).

The project declares GPL-3.0-only; Prost declares Apache-2.0. Apache documents GPLv3 compatibility, so this is not an apparent licensing blocker; exact selected dependencies/tools and notice retention still need normal review. [Prost manifest](https://github.com/tokio-rs/prost/blob/v0.14.4/Cargo.toml), [Apache guidance](https://www.apache.org/licenses/GPL-compatibility).

Priority order: retain formats; profile; evaluate protobuf on a new external interface if introduced; consider a disposable binary cache before migrating the journal.
