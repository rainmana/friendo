Build the first usable version of **friendo** in this workspace. You are the lead Rust engineer and interaction designer. This is an implementation task: inspect the relevant code, make a short plan, write the application, run it, inspect its behavior, and fix it until the first milestone works.

friendo is a native, cross-platform, general-purpose agent harness for expert users: developers, security engineers, researchers, and people who want direct control. Its interface is a Ratatui terminal with an extravagant cyberpunk identity. Its interaction model is grounded in the Meta-Context Window framework.

The defining experience is a powerful, configurable intelligence inside a terminal that feels like it is breaking out of containment. Coordination repair feels like tracing and rewiring a live system. The visuals respond to real execution and evidence. The user can choose YOLO autonomy when they want less intervention.

Deliver a runnable application with a complete narrow workflow. A plan, scaffold, static dashboard, or simulated animation alone does not complete this task.

# Working agreement

Work in the intended friendo checkout when present. Read applicable AGENTS.md instructions and inspect the existing working tree before editing. Preserve unrelated work. If this is an empty workspace, initialize friendo. If the current repository is unrelated, use a clearly isolated friendo directory within the authorized workspace.

Make reasonable, reversible implementation decisions and record important ones briefly. Continue through implementation and validation without asking me to approve routine steps. Ask only when essential information or authority is genuinely missing, after completing independent work.

Delegate independent tasks when subagents are available, with clear file ownership and integration points. Keep one agent responsible for the end-to-end application. Coordinate through shared types before parallel edits.

Use current primary documentation and compatible dependency versions. Keep a lockfile. Do not spend the implementation on a broad framework survey.

Keep this work local. Do not publish, push, deploy, change account settings, or alter the coding environment’s permissions as an incidental part of the build. The product’s YOLO mode is an application feature, not an instruction to bypass your own execution environment.

Avoid automatic model downloads and GPU workloads. Develop using a deterministic demo and mocked provider fixtures; run small live checks only against configured providers. Missing credentials must not stop the rest of the build.

# Product identity

Use the name **friendo** in lowercase.

Build a serious instrument with an intense visual identity: advanced intelligence, fictional cyberweapon-console energy, escaped containment, expensive-looking terminal surfaces, and an unsettlingly friendly name. Make it visually distinctive from the first running version.

The product is general purpose. Security workflows are an excellent demonstration, not the only supported domain. Support ordinary development, analysis, research, and command-line work through the same primitives.

Expert operation means direct controls, inspectable state, predictable configuration, and concise failures. Avoid compulsory onboarding, repeated permission theater, hidden policy changes, and arbitrary puzzles before real work.

Let users edit prompts, model settings, context rules, tool behavior, execution policy, workflows, keybindings, layout, and effects through clear extension points. Implement a useful subset now and document remaining controls honestly.

Treat personality as configurable presentation. Keep command text, errors, evidence, and status technically precise even when the surrounding interface is theatrical.

# Canonical sources and reuse

Read these before defining MCW semantics:

- [MCW framework](https://github.com/rainmana/mcw-framework)
- [Canonical glossary](https://github.com/rainmana/mcw-framework/blob/main/docs/glossary.md)
- [MCW constitution](https://github.com/rainmana/mcw-framework/blob/main/docs/constitution.md)

Record the source revision consulted. Preserve canonical terminology and label friendo-specific operational mappings as application design. If the sources cannot be fetched, use the requirements in this prompt, identify the missing verification, and continue independent implementation.

Inspect [Graffy](https://github.com/rainmana/graffy) for reusable journal, graph, coordination-record, and execution components. Assess actual code and tests rather than assuming the README establishes production readiness. Reuse selectively when that reduces complexity. Preserve applicable attribution and license obligations. Do not adopt its architecture wholesale or rename it as a substitute for implementing this product.

Starting technical references:

- [Ratatui](https://ratatui.rs/)
- [TachyonFX](https://github.com/ratatui/tachyonfx)
- [Crossterm](https://github.com/crossterm-rs/crossterm)
- [portable-pty](https://docs.rs/portable-pty/)
- [genai](https://github.com/jeremychone/rust-genai)
- [Rig durable runs](https://rig.rs/docs/concepts/runs/)
- [Rust process execution](https://doc.rust-lang.org/std/process/struct.Command.html)
- [Radare2 r2pipe](https://book.rada.re/scripting/r2pipe.html)

Use provider-specific official documentation for the APIs actually implemented.

# First milestone and completion condition

Finish one coherent vertical slice containing all of the following:

1. A native Rust executable with the real Ratatui interface and a cohesive cyberpunk theme.
2. A clearly labeled offline demonstration that exercises the same state and rendering paths as live operation.
3. A real streaming model and tool loop, configurable without recompilation.
4. A generic OpenAI-compatible adapter suitable for a configured local endpoint, plus one native hosted-provider adapter. Prefer Anthropic for the second protocol unless repository evidence makes another choice substantially cheaper.
5. Direct CLI execution and at least one persistent process-session path.
6. A durable event journal, output artifacts, context inspection, and historical review.
7. A useful live topology map built from recorded entities and relationships.
8. An interactive correction-and-repair workflow with satisfying event-driven effects.
9. An explicit YOLO autonomy policy.
10. Responsive interruption, terminal cleanup, and meaningful verification.

Implement modules in one crate or a small workspace. Choose the simplest structure that preserves the boundaries below.

Defer distributed swarms, a plugin marketplace, automatic network discovery, universal filesystem rollback, live-session migration between machines, exhaustive provider parity, and complex reward economies. Create practical seams for them without building placeholder subsystems.

Every visible control in the delivered milestone must work or be clearly identified as unavailable. Do not present planned capabilities as implemented.

# Runtime architecture

Separate these responsibilities:

- **Run controller:** schedules model calls, tool actions, interruptions, corrections, and permitted repairs.
- **Provider adapters:** encode requests, decode streaming events, preserve provider-native continuation information, and report capabilities.
- **Process and session runner:** executes commands and owns their lifetimes.
- **Journal and artifact store:** retains authoritative events and output.
- **Coordination logic:** evaluates explicit revisions and dependencies, records interpretations, and manages repair.
- **TUI:** renders state and emits user actions.
- **Headless entry point:** exercises the same controller for tests and scripted use.

Use typed events with stable IDs, run and branch IDs, per-run sequence numbers, source attribution, causal references, and a schema version. Derive inspectable state through a reducer or comparably explicit transition model.

Keep the executable plan, evidence relationships, and visual graph distinct. The topology is a projection; drawing an edge never grants execution authority or proves its meaning.

Separate authoritative persistence from disposable rendering notifications. Coalesce redraws and streamed presentation updates where helpful; never silently discard corrections, tool outcomes, or artifact references. Bound memory and output queues. Rendering backpressure must not block cancellation.

A versioned append-only JSONL journal with Serde is sufficient initially unless an existing component demonstrably improves the implementation. Store large outputs separately and reference them. Avoid adding a graph database merely to draw a graph.

# Native CLI tooling

CLI execution is the primary tool substrate. MCP remains an optional adapter direction; no MCP server is required for the first useful workflow.

Implement distinct operations for:

- **Direct invocation:** executable, argument array, working directory, stdin, explicit environment overrides or references, timeout, and output policy.
- **Shell evaluation:** an explicitly selected shell for pipelines and shell-dependent expressions.
- **Persistent sessions:** start, read, write, interrupt, wait, and terminate.

Use argument arrays for direct execution. Do not silently feed every tool request to Bash. Support native Windows and its shell conventions as well as Unix systems.

Retain tool versions when available, invocation metadata, exit status, termination status, output artifacts, and the exact excerpts delivered to the model. Drain stdout and stderr concurrently. Spool large output rather than growing memory indefinitely.

A persistent r2 or debugger session must survive individual model turns. Prefer native pipes or an existing tool protocol when appropriate; use a PTY for genuinely interactive sessions. r2pipe is compatible with this CLI-first design.

Supervise process descendants with platform-appropriate facilities. Distinguish cancellation requested from termination observed and report incomplete termination honestly. Restore the terminal on normal exit, error, and panic.

Sanitize noninteractive output for the main UI so control sequences cannot rewrite application chrome or manipulate the clipboard. Interpret interactive terminal sequences only inside an isolated terminal component with a defined feature policy.

Treat tool output as attributed data. It cannot silently become user configuration or enable YOLO.

# Providers and configuration

Start with real streaming text and tool calls. Accumulate fragmented tool arguments, validate the complete request, and dispatch only after the call is complete. Preserve tool-call IDs and associate results correctly.

Expose support for tools, structured output, images, reasoning controls, caching, usage reporting, and continuation as separate capabilities. Report unsupported features explicitly. Keep provider-specific options and opaque continuation fields where needed.

Evaluate genai or selected Rig components as transport dependencies. Friendo owns its execution policy and coordination transitions. Do not delegate those semantics to a library’s default agent loop.

Use configurable provider aliases, endpoints, model IDs, and credential references. Do not hard-code guessed current model names. Show missing usage or pricing as unknown; never display invented zero cost.

Provide an effective-configuration inspector: value, source, override precedence, and the boundary at which a change applies. Display the active provider, model, execution policy, and visual preset.

The context inspector shows the actual outbound content available to friendo, including selected artifacts and transformations. Mark provider-internal context as unavailable rather than claiming full visibility.

Keep credentials out of journals, exports, test fixtures, and screenshots. Store references instead of keys, headers, or whole environment snapshots. Preserve source identity for imported files, tool output, and direct user instructions.

# MCW semantics

MCW means **Meta-Context Window**. It concerns emergent coordination between participants. Friendo records observations that support and study those dynamics; it does not store MCW itself, inspect a human’s private understanding, or compute a validated universal alignment percentage.

Keep distinct records for exact user statements, agent interpretations, model assessments, mechanically checked conditions, tool observations, claims, supporting artifacts, and explicit human acknowledgments.

Preserve the canonical failure taxonomy: drift, asymmetric state advancement, false alignment, overcompression, constraint opacity, and repair suppression. Detectors must identify their basis as mechanical check, model assessment, or human report. A possible failure is not a proven diagnosis.

A correction creates a new relevant intent revision. Mark known dependent plans and answers stale while retaining prior events and artifacts. Record whether each dependency was declared, mechanically derived, or model-proposed. Do not claim exhaustive impact detection.

Old evidence can remain factually valid while its relevance to a revised answer changes. Preserve it and make that distinction visible.

Track correction recorded, context delivered, interpretation updated, and checks completed as separate transitions. An acknowledgment is never inferred from a panel being displayed.

When revisions affect concurrent work, re-evaluate queued dependent actions before dispatch. Identify already-running actions and apply the configured interruption policy. Independent work can continue. Never imply a correction retroactively prevented an action.

Implement the useful mechanically checkable subset first, including revision mismatch and stale explicit dependencies. Preserve uncertainty for semantic comparisons. H/R/D/M, if exposed later, remain labeled ordinal research proxies with rater and method provenance.

# Rewire interaction and rewarding repair

Make coordination repair feel like hacking a live system through meaningful operations.

Pressing a configurable repair key on an affected node opens **Rewire**. Show the exact instruction and source, current interpretation, conflict or stale dependency, proposed change, and affected work. Let the user edit or select a correction and apply it directly.

Support these actions, with the canonical operation recorded in the inspector:

- **Ground:** reintroduce the current goal, constraints, and sources; maps to re-grounding.
- **Expand:** reopen retained source material and recover omitted distinctions; maps to decompression. If the source is absent, expose that gap.
- **Prioritize:** revise explicit priority weights or ordering and show affected planning choices; maps to re-weighting.
- **Split:** separate competing meanings and select or edit the intended interpretation; maps to disambiguation.
- **Sync:** deliver recorded revisions and expose the updated interpretation; maps to synchronization.

Add fast source tracing and constraint pinning. Pinned content must have a visible inclusion policy and an observable result in the context inspector.

Make feedback immediate and satisfying:

- Stale connections split into amber segments.
- Tracing a cause illuminates its actual dependency path.
- Applying a revision sends a bright sweep through affected nodes.
- Updated dependencies reconnect.
- Successful declared checks produce a short specular highlight and a concise receipt.

Example receipt:

`REVISION APPLIED / 3 DEPENDENCIES REFRESHED / 1 CHECK PENDING`

Emit `LINK RESTORED` only when its documented technical conditions hold. Clicking it reveals those conditions and their evidence. It is a statement about recorded revisions and checks, not proof of identical human and model understanding.

Reward discovering a useful mismatch as well as resolving it. Give quiet positive feedback when a meaningful check verifies that an explicit constraint survived a handoff or context change. Tie each reward to an event; idle time and absent detector alerts do not prove maintained alignment.

Use local visual feedback, precise receipts, and optional session counts initially. Do not punish corrections, erase a streak when uncertainty is raised, or reward hiding conflicts. Do not invent drift to create gameplay.

There is no mandatory puzzle, timing challenge, or animation delay before real work. Every repair remains fast and functional with effects disabled.

# YOLO autonomy

Implement an explicit user-selected autonomy mode named **YOLO**, independently configurable from logging and visual effects.

Normal coordination mode surfaces material unresolved interpretation conflicts and pauses only dependent work that needs the user’s intent. Ordinary operations follow the configured execution policy without constant prompts.

YOLO suppresses discretionary confirmations, lets the agent make reasonable bounded assumptions, and permits automatic repairs under the selected execution policy. Record assumptions and unresolved conflicts while useful work continues. Keep the same evidence, history, inspectors, and interruption controls.

Preserve explicit task constraints until the user changes them. If an action conflicts with one, choose a compatible action or report the affected action as blocked. Do not silently rewrite the constraint.

Only direct user action or user-controlled configuration can change autonomy. Model output, tool output, imported files, and retrieved text cannot switch it.

Display `AUTONOMY: YOLO` persistently and record mode changes with their effective boundary. Provide a quick way to leave the mode. No recurring warning modal is required.

The mode operates within the actual permissions of the selected executor and provider. It does not disable operating-system protections, bypass provider restrictions, erase unknown outcomes, or convert unverified evidence into success.

# Live topology map

Make the map a primary working view with a real network-console aesthetic. Use typed nodes, typed edges, stable identities, and inspectable provenance.

Provide lenses over the same recorded session:

- **Runtime:** agents, provider endpoints, model requests, tool processes, and persistent sessions.
- **Coordination:** intent revisions, interpretations, plans, corrections, and affected dependencies.
- **Evidence:** artifacts, excerpts, claims, and checks.

Represent relationships such as `requested`, `invokes`, `produced`, `depends_on`, `supports`, `supersedes`, and `delivered_to`. Every factual edge references a source event. Label proposed or inferred links.

Keep domain network topology distinct. Define host, service, and observed-link records that can later come from CLI parsers. Include a small, explicitly synthetic imported fixture in the demo so the renderer supports an actual host/service view; do not claim live discovery or automatically scan a network.

Required interactions: keyboard selection, inspector access, pan and zoom, focus on selected or active work, collapse and expand groups, filter by node or edge type, trace upstream evidence, and highlight downstream impact. Provide a list alternative.

Use a stable layout that preserves spatial orientation as events arrive. Stream activity along existing edges; do not continuously bounce the whole map. Show selected labels at readable size and preserve selection across updates.

Historical review must show the graph at the selected recorded position. Reviewing old events must never execute tools. Synthetic fixtures remain visibly identified throughout their views and exports.

# Visual design and terminal behavior

Use Ratatui, Crossterm, and TachyonFX or a demonstrably compatible equivalent. The application and its control logic are Rust. No Electron wrapper, browser dashboard, Node runtime, or Python service is required to operate the product.

Default art direction: near-black surfaces; electric cyan execution accents; magenta interpretation accents; amber unresolved-state accents; terminal gradients; bright edge highlights; beveled cell patterns; sharp geometry; restrained scanline textures; short chromatic displacement; and flowing topology traces. Simulate gloss through color and character treatment within the terminal.

Build an original coherent visual system. Keep technical content stationary and copyable while surrounding regions animate. Status must have labels or symbols in addition to color.

Provide a skippable startup flourish with the escaped-containment atmosphere. Theme copy must not falsely report that permissions, sandboxing, or network access changed.

Support adjustable effects including **off, reduced, full, and overdrive**. Make the default visibly ambitious. Keep reduced-motion and reduced-color operation complete. Do not depend on a special font or terminal image protocol.

Support ordinary terminal resizing and a useful small-window layout. The interface needs a command composer, intent/interpretation view, execution/evidence view, topology, and event timeline; these can be navigable views rather than five permanently crowded panels.

Use an initial active-animation budget around 30 FPS, configurable. Idle rendering reacts to events. Profile before claiming performance. Favor local effects over constant full-screen redraws, avoid strobing, and never block input behind animation.

Use keyboard-first navigation with remappable bindings and discoverable contextual help. Define cancellation and quit behavior clearly; quitting must not silently orphan active tools. Optional mouse interaction complements the keyboard path.

# Persistence and recovery

Persist enough information to reconstruct the observed run: source revisions, effective non-secret configuration, prompts or their retained references, selected context, transformations, invocations, output artifacts, corrections, and checks.

Define retention and redaction explicitly. If source text is removed or transformed, expose that fact and limit replay claims accordingly.

Implement historical review now. Keep the meanings of **review, resume, rerun, and fork** distinct. Resuming uses the current environment. Forking recorded context does not restore external state without an explicit snapshot facility.

Recover valid records before a truncated journal tail and report the damaged tail. Never truncate an existing run accidentally.

After a crash around external execution, preserve **outcome unknown** when there is no reliable completion evidence. Do not automatically replay a possibly completed side effect. Durable records do not establish exactly-once external execution.

Keep process state, provider errors, and incomplete stream capture visible. Never invent successful completion to make the interface appear coherent.

# Implementation sequence

First, perform a short repository and dependency inspection. Write a concise plan with decisions and acceptance checks, then start implementing immediately.

Use this sequence unless the repository provides a clearly better path:

1. Build the event model, reducer, journal/artifact seam, and a runnable themed TUI with a labeled demo.
2. Connect real provider streaming, direct process execution, output capture, and a persistent session to that same runtime.
3. Complete intent revision tracking, Rewire, effects, topology inspection, and YOLO.
4. Verify interruption, journal recovery, credential handling, terminal behavior, and the integrated demonstration. Repair failures before calling the milestone complete.

Keep the application runnable between checkpoints. Do not stop after the visual fixture; live execution is part of the milestone.

Create concise durable project files:

- `README.md` for operation.
- `docs/product.md` for this specification.
- `docs/architecture.md` for important decisions.
- `docs/build-status.md` for completed work, verification, blockers, and the next bounded step.

Keep ongoing instructions in the appropriate existing project guidance without overwriting unrelated rules.

If time or environment limits interrupt the run, leave an accurate checkpoint, a runnable subset when possible, and an exact next action. Do not claim the whole milestone is done.

# Verification

Use meaningful tests across boundaries. A small Rust fixture executable can emit output, spawn descendants, wait, and support interruption without requiring Bash or Python.

Verify:

1. Fragmented streamed tool arguments never execute until complete and validated.
2. A correction creates a revision, stales affected dependent work, preserves old artifacts, and leaves independent work available.
3. Rewire changes actual state and its feedback follows the corresponding events.
4. YOLO changes discretionary intervention while retaining explicit constraints, uncertainty, history, and source attribution.
5. Model and tool content cannot silently enable YOLO or rewrite effective user configuration.
6. Large stdout and stderr are captured with bounded memory while input and cancellation remain responsive.
7. Interruption reports observed process and descendant status on each tested platform.
8. Historical review performs no model calls or tool execution; a truncated journal preserves earlier valid records.
9. Recovery around ambiguous action completion retains outcome unknown without silently duplicating the action.
10. Context and evidence inspectors expose transformations, source revisions, and the actual excerpts used.
11. Credentials are absent from journals, exports, fixtures, and captures.
12. Topology selection remains stable; nodes and edges reveal their source; fixture data stays labeled.
13. Resizing, reduced motion, reduced colors, and normal terminal exit preserve essential information and controls.
14. An effect cannot delay interruption or create a success event.

Run `cargo fmt --all --check`, appropriate `cargo clippy` checks, and the meaningful test suite. Configure Windows, macOS, and Linux CI. Report which platforms were actually executed; a workflow file or successful cross-compilation is not a passed native runtime test.

Inspect the running TUI visually at normal and narrow sizes when the environment allows it. Capture a short terminal recording or screenshots if available. If interactive inspection is unavailable, exercise the Ratatui test backend and say precisely what remains visually unverified.

For provider adapters, use deterministic HTTP fixtures and a small live smoke test when credentials are available. Keep mocked and live results separate.

Broaden testing only to resolve a concrete remaining risk. Fix observed failures rather than accumulating ceremonial tests.

# Demonstration and final handoff

Ship a credential-free simulated demonstration with this flow:

1. Open a symbol-inventory task for a synthetic local binary or fixture and show its topology.
2. Reveal an invocation, output artifact, and supported answer.
3. Change the goal to exported symbols only while preserving the original-binary constraint.
4. Highlight the previous interpretation and affected answer.
5. Open Rewire, inspect the mismatch, and apply the correction.
6. Show delivery, interpretation update, refreshed evidence, and check completion as separate events.
7. Play the earned reconnection effect and show a precise repair receipt.
8. Switch to YOLO; the mode indicator changes while evidence and unresolved states remain available.
9. Review an earlier point without executing anything.

Also demonstrate a live task against a configured provider that invokes a real harmless local command and produces inspectable output. If no live provider is available, complete and test the integration with fixtures and mark live verification pending.

Provide working commands equivalent to:

- `friendo demo`
- `friendo run`
- `friendo runs`
- `friendo replay <run-id>`
- `friendo doctor`

Document the actual syntax you implement. Expose YOLO through a session control and an explicit CLI/configuration option.

Finish with the implemented capabilities, exact build/run commands, a short repair/topology walkthrough, tests actually run, platform verification, limitations, and the next narrow milestone.

**Begin now. Inspect the workspace, choose a concrete first path, and build the runnable vertical slice.**