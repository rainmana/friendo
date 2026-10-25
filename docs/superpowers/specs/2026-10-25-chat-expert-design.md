# Chat and Expert presentation modes

## Approved direction

The user requested a simpler interface for people familiar with Claude Code/ChatGPT while preserving friendo's visual fidelity, map/topology, and an inspectable reasoning/understanding layer. They approved the visible **Chat + live map** mockup on 2026-10-25. The existing console remains **Expert**. The Shared stage mockup is an alternative study rather than a second implementation target.

## Experience

- Chat is conversation-first, with a persistent composer, attributed user/model messages, expandable command/results, and a compact live topology. Cyan/magenta/amber, rounded panels, and existing event-driven effects remain shared with Expert.
- An Understanding panel shows recorded task intent, model interpretation, observed tool evidence, unresolved conflicts, and current status. It does not invent hidden reasoning, certainty, agreement, or a plan absent from recorded content.
- Wide terminals show chat beside the map/Understanding. Narrow terminals retain a compact map/activity area with chat; inspectors carry details that do not fit.
- The current dense console remains available through F2 and `--ui-mode expert`. Mode switching preserves the same run, journal, active operation, history position, and text draft.
- Chat starts with text input focused. Ordinary letters, including q/y/r, are text in the composer. Enter submits, paste supports Unicode/newlines, Esc leaves input, and an explicit key/control re-enters it. Ctrl-C interrupts the existing controller. Quit remains discoverable outside input.
- Command cards pair ToolStarted/ToolFinished by identity and expose observed stdout/stderr, exit/capture/termination state, and provenance. Missing completion remains pending live work or outcome unknown in historical/standalone review.
- Correct direction opens a compact correction editor backed by existing Rewire semantics; explicit constraints and earlier evidence are preserved. Expert keeps its full Rewire interface.
- Details/context/evidence remain inspectable without making their technical vocabulary the primary path.

## Boundaries

- UI mode is presentation only, independent of NORMAL/YOLO and map/list layout. Switching it does not append journal events, invoke tools/models, or alter execution policy.
- Chat cards project typed journal events, not the truncated model-only transcript. Historical rendering uses the same event prefix and remains read-only.
- Streaming preview is disposable and labeled separately from retained completed answers.
- Existing provider, execution, process, journal schema, correction, cancellation, and credential handling stay unchanged. No new dependencies are required.
- Add `ui_mode = "chat" | "expert"` with default chat, validation/inspector support, and a CLI override. Keep root friendo.toml unchanged so the installed v0.1.0 binary can still parse its existing configuration. The new example config may document ui_mode.
- Keep the installed published v0.1.0 binary and running user session unchanged. This work is local/unreleased until explicitly published.

## Verification

Test event ordering/attribution, exact user text, tool pairing and incomplete outcomes, old/repeated model answers, input focus and Unicode paste, mode toggles without authority changes, draft/history preservation, remaps, read-only controls, and narrow/off/no-color rendering. Extend native terminal QA to exercise Chat and Expert with the same real local HTTP/process fixture. Run the complete local preflight and inspect actual rendered frames before completion.

## Implementation calls

Default Chat, F2 switching, F3 details, F4 correction, and responsive layout thresholds are implementation choices based on the approved visual direction. Preserve existing inspector and Expert shortcuts when their views have focus. New shortcuts must appear in help/footer, and displayed controls must work.
