# Chat and Expert Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** Deliver the approved conversation-first Chat mode with live map/Understanding and preserve the current console as Expert.

**Architecture:** Both modes render the same event prefix and controller state. A pure conversation projection pairs typed journal events; input focus and mode remain disposable View state. Configuration only chooses the initial presentation.

**Tech Stack:** Existing native Rust, Ratatui/Crossterm, Serde; Python remains QA only.

**Spec:** ../specs/2026-10-25-chat-expert-design.md

## Global Constraints

- Preserve NORMAL/YOLO, execution policy, credential handling, existing provider/process/controller/journal semantics, and installed v0.1.0.
- Do not add ui_mode to root friendo.toml; old installed releases reject unknown fields.
- Mode changes and historical review never authorize execution or append events.
- Keep credentials, generated runs, screenshots/recordings, and index state out of commits. Do not push/publish this feature as an incidental step.

## Review Focus

- User types q/y/r, Unicode or pasted newlines: text input must not trigger global authority controls.
- Tool has no reliable completion: history must retain outcome unknown rather than display success.
- Mode switch while editing/streaming/reviewing: preserve draft, operation, review prefix, and autonomy.
- Small terminals and long outputs: essential controls remain readable, content is sanitized/bounded, and no geometry panic occurs.
- Old journals/configurations: missing ui_mode defaults safely; read-only review remains compatible.

### Task 1: Typed conversation projection

**Files:** Create src/conversation.rs; root integrates module in src/lib.rs.

**Interfaces:** `pub fn project(events: &[Event]) -> Vec<ConversationCard>`; `CardKind::{User,Correction,Assistant,Tool,Notice}`; cards expose id, source_event, outcome_event, revision, kind, text, invocation/result JSON options.

- [x] Add failing behavior tests for exact intent/correction/model ordering, matching tool outcomes by ID, historical missing completion, and attribution.
- [x] Implement the pure event projection without authority, replay, I/O, or model calls.
- [x] Run focused tests; report red/green evidence to the integrator.

### Task 2: Initial mode configuration and CLI

**Files:** src/config.rs, src/main.rs, tests/configuration.rs, tests/cli.rs, friendo.example.toml.

**Interfaces:** `Config.ui_mode: String` accepts chat/expert, default chat; global CLI `--ui-mode` overrides file and is recorded in the config inspector. Root consumes recorded fields.ui_mode.value when initializing View.

- [x] Add failing tests for default/file/CLI precedence, unknown mode rejection, and old configuration compatibility.
- [x] Implement the field/inspector/CLI and example documentation; do not edit root friendo.toml.
- [x] Run focused configuration/CLI tests and preserve existing provider/autonomy assertions.

### Task 3: Chat rendering, focus, and Expert integration

**Files:** src/tui.rs; optionally focused src/chat.rs renderer; src/lib.rs.

**Interfaces:** Existing render signature and controller actions remain; View gains initial presentation, Chat selection/expanded-card/focus state, and bounded disposable streaming preview if needed.

- [x] Add failing tests for safe text input, F2 draft/history/autonomy preservation, read-only mode changes, and selected-card details.
- [x] Implement Chat rendering with shared topology/palette/panels, attributed Understanding, persistent composer, and compact correction/details controls. Keep existing Expert rendering/operations intact.
- [x] Cover wide/narrow/tiny/off/no-color rendering and pending/failed/incomplete tool labels with TestBackend.

### Task 4: Native journeys and review

**Files:** tests/terminal_smoke.py, README.md, docs/testing.md, docs/build-status.md, design/plan progress.

- [x] Adapt old Expert journeys to request expert explicitly; add a Chat journey using local HTTP and real fixture process, Unicode input, safe typing, mode switching, corrections, read-only review, interruption, and terminal restoration.
- [x] Run `bash scripts/check.sh`; run actionlint if workflows change; inspect actual wide and narrow rendered frames.
- [x] Request independent review, address findings, and rerun only checks justified by changes.
- [x] Leave a concrete tested local build and accurate verification report; do not replace installed release or publish without user direction.

## Completion evidence

All four tasks are complete locally. The final `bash scripts/check.sh` passed on macOS/aarch64: 101 Rust tests, formatting, Clippy, doctest check, binary build, 7 release guards, both terminal journeys and the added standalone replay session. Independent review identified and verified fixes for map/correction draft loss and hidden operation selection; long-editor visibility was also corrected. Actual wide/narrow rendered frames were inspected. See [build status](../../build-status.md#chatexpert-local-pre-push-verification--2026-10-25) for scope and evidence. The separate preview is running; installed v0.1.0, root TOML, credentials, Protobuf, and existing user session remain unchanged. This completion evidence describes the local checkpoint before feature commit, push, or hosted CI; those subsequent actions must be verified against their commit and Actions run.
