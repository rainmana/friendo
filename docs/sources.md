# Sources and terminology

Consulted 2026-10-25. Repository references below pin the **commit**, rather than a file's Git blob SHA. Dependency versions actually selected for the application are in `Cargo.lock`.

## Canonical MCW sources

W. Alec Akin, *Meta-Context Window Framework*, Apache-2.0, commit [`8365d220f2676f248c934e20f23e427e01cf3ce8`](https://github.com/rainmana/mcw-framework/tree/8365d220f2676f248c934e20f23e427e01cf3ce8):

- [Framework overview](https://github.com/rainmana/mcw-framework/blob/8365d220f2676f248c934e20f23e427e01cf3ce8/README.md).
- [Canonical glossary](https://github.com/rainmana/mcw-framework/blob/8365d220f2676f248c934e20f23e427e01cf3ce8/docs/glossary.md), including amendments and exclusion boundaries.
- [Constitution](https://github.com/rainmana/mcw-framework/blob/8365d220f2676f248c934e20f23e427e01cf3ce8/docs/constitution.md), including the v1.1 Article III amendment.

MCW means **Meta-Context Window**: emergent coordination through interaction between human and artificial context windows. A saved transcript, memory store, context inspector, or graph records observations; none stores MCW or exposes a person's private understanding. The canonical framework remains exploratory, with empirical claims at L0–L1 and no quantitative validation. friendo makes no claim that its interface improves measured MCW dynamics.

The canonical failure names are drift, asymmetric state advancement, false alignment, overcompression, constraint opacity, and repair suppression. A stale revision is a mechanically detectable condition; it does not establish a semantic diagnosis. H/R/D/M are qualitative ordinal research proxies, not a universal alignment percentage, and are not scored by this milestone.

The following are **friendo application mappings**, preserving canonical operation names without redefining them:

| Rewire label | Canonical operation | Application operation |
| --- | --- | --- |
| Ground | Re-grounding | Reintroduce goal, constraints, and retained sources. |
| Expand | Decompression | Reopen retained source distinctions; expose absent source material. |
| Prioritize | Re-weighting | Change explicit priority ordering. |
| Split | Disambiguation | Separate competing interpretations and record the selected meaning. |
| Sync | Synchronization | Deliver a recorded revision and record the updated interpretation separately. |

Recorded delivery, interpretation update, mechanical checks, and explicit human acknowledgment are different observations. A repair receipt concerns those records and their revision dependencies; it cannot prove identical understanding. The topology projects recorded relationships and grants no execution authority. These are implementation decisions, not new MCW definitions or validated research results.

## Reuse and transport decision

[Graffy source assessment](graffy-assessment.md) records concrete files, assertions inspected, the pinned revision, and the failed test attempt. No Graffy source is vendored or linked by this milestone.

[genai](https://github.com/jeremychone/rust-genai) was evaluated from its primary README: it offers native provider adapters, streaming, custom OpenAI-compatible endpoints, and provider-specific continuation support. Its then-current documentation identifies release 0.7. It would be a reasonable future transport when broader provider coverage is needed. This is documentation review, not a local genai integration test.

[Rig durable runs](https://rig.rs/docs/concepts/runs/) documents a serializable `AgentRun` stepping protocol (`CallModel`, `CallTools`, `Done`) with a separate I/O driver. That separation is useful, but adopting its persisted state would add a second run schema alongside friendo's authoritative events. Rig's run loop and hooks do not define friendo's correction, dependency, or autonomy semantics.

The selected narrow transport uses `reqwest` directly for two protocols. This keeps exact request capture, fragmented tool argument validation, native content blocks, stream completion, cancellation, and usage absence explicit at friendo's adapter boundary. It also makes friendo responsible for SSE correctness and provider protocol maintenance; fixture tests are required. This decision does not imply genai or Rig cannot satisfy these requirements.

## Primary technical references

- [OpenAI Chat Completions streaming events](https://developers.openai.com/api/reference/resources/chat/subresources/completions/streaming-events): indexed tool-call fragments and optional final usage. Compatibility here means this protocol subset, not every OpenAI API or every local server.
- [Anthropic streaming Messages](https://platform.claude.com/docs/en/build-with-claude/streaming): content block lifecycle, JSON argument fragments, stop events, and usage. Configured model IDs are user supplied.
- [Ratatui](https://ratatui.rs/), [Crossterm](https://github.com/crossterm-rs/crossterm), and [TachyonFX](https://github.com/ratatui/tachyonfx): terminal rendering, events, restoration, and presentation effects.
- [Rust `std::process::Command`](https://doc.rust-lang.org/std/process/struct.Command.html): executable and argument-array process semantics.
- [portable-pty](https://docs.rs/portable-pty/0.9.0/portable_pty/): cross-platform PTY seam for genuinely interactive programs. Native pipe sessions remain useful for line-oriented tools.
- [Radare2 r2pipe](https://book.rada.re/scripting/r2pipe.html): future protocol-backed persistent binary-analysis sessions; listing this reference does not claim an r2 integration was tested.

Live provider and platform verification belong in `build-status.md`; reading documentation, compiling a dependency, and passing a protocol fixture are distinct from live service validation.
