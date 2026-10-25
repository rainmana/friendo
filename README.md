# friendo

The best agentic harness you didn't ask for :D

A native Rust agent console with durable evidence, direct CLI tools, and an extravagant Ratatui interface. Near-black surfaces, cyan execution traces, magenta interpretations, and amber stale dependencies make the recorded run inspectable. Personality is presentation; command output and execution status stay precise.

This first slice includes a credential-free guided demo, OpenAI-compatible and native Anthropic streaming adapters, direct execution, persistent pipe sessions, revision repair, topology, and read-only historical review. See [build status](docs/build-status.md) for checks actually completed and remaining limits.

## Build and start

Install a current stable Rust toolchain, then:

```console
cargo build --locked --bins
cargo run --locked -- demo
```

The built executable is `target/debug/friendo` on macOS/Linux and `target\debug\friendo.exe` on Windows. Commands below use `friendo` as shorthand for that executable; `cargo run --locked -- <arguments>` also works. No Node runtime, Python service, GPU workload, or model download is needed.

```console
friendo doctor
friendo demo --headless
friendo exec whoami
friendo runs
friendo replay <run-id>
friendo replay <run-id> --headless --at 20
friendo replay <run-id> --context
```

`whoami` must exist on the host. `doctor` inspects configuration and credential presence without contacting a model. `--at N` reviews the first N recorded events; `--context` prints the last retained outbound body at that position.

## Repair walkthrough

1. Run `friendo demo`. Every simulated artifact and domain fixture remains labeled **SYNTHETIC**; the demo does not execute a binary or scan a network.
2. Press **Space** to reveal the fixture invocation, two-symbol output artifact, and supported answer. Use **Tab** to cycle Runtime, Coordination, Evidence, and Domain lenses.
3. Press **Space** again: the goal becomes exported symbols only. The original-binary constraint stays. The old interpretation, plan, and answer become stale.
4. Press **r** for **Rewire**. Inspect the source, prior interpretation, and affected work. Keep the supplied exported-only correction unchanged; **Tab** selects Ground, Expand, Prioritize, Split, or Sync. Press **Enter** to apply. This guided fixture accepts its specific correction; arbitrary goal edits belong in `run`.
5. Delivery, updated interpretation, refreshed evidence, and checks appear as separate events. A reconnection effect accompanies the recorded receipt. The old answer stays inspectably stale.
6. Press **y** to enter or leave **YOLO**. Press **[** to review an earlier position; **]** past the end or **Esc** returns to the current view. Review never dispatches actions.

The demo's `LINK RESTORED` receipt means its current revision was delivered, its interpretation updated, and the two declared fixture checks passed. In Intent view, **Enter** reveals the conditions. It is not proof of shared human/model understanding. Live repair records revision checks without promoting semantic correctness to success.

## Work with a provider

Copy [friendo.example.toml](friendo.example.toml) to `friendo.toml`, or pass `--config <path>`. Set an endpoint and an explicit model ID available from that endpoint. A minimal local configuration is:

```toml
active_provider = "local"

[providers.local]
protocol = "openai"
endpoint = "http://127.0.0.1:8080/v1"
model = "YOUR_CONFIGURED_MODEL_ID"
max_tokens = 2048
```

For Anthropic, configure an alias with `protocol = "anthropic"`, `endpoint = "https://api.anthropic.com/v1"`, and `api_key_env = "ANTHROPIC_API_KEY"`. Put the key in that environment variable, never in TOML. A local server may omit `api_key_env` when authentication is unnecessary. Configure only aliases with valid model IDs; all provider entries are validated.

```console
friendo doctor
friendo run "Run exec with executable whoami and args [], then report the observed output."
friendo run "Inspect the project without changing files" --constraint "Do not modify files"
friendo run "Report the current user through exec" --provider local --headless
friendo --yolo run "Report the current user through exec"
```

Omit the goal in `friendo run` to open the composer. Use **i**, type a task, and **Enter**. `/conflict <text>` records a possible human-reported interpretation conflict; Normal mode pauses dependent work. `/continue` starts another turn using the current in-memory context, including recorded retained-session IDs/status. It does not resume a run from disk. Rewire writes a correction and starts a new turn when idle; after interrupting an active turn, submit `/continue` once it stops.

Composer controls also include `/ack` for an explicit acknowledgment of the current recorded interpretation, `/constraints ["Do not modify files"]` to replace the full constraint list in a new revision, and `/report false_alignment <text>` to record a possible canonical failure with human-report provenance. Other accepted report names are `drift`, `asymmetric_state_advancement`, `overcompression`, `constraint_opacity`, and `repair_suppression`. None of these reports establishes a proven diagnosis.

Submit a real tool directly without a provider using `/tool {"id":"manual-1","name":"exec","arguments":{"executable":"whoami","args":[]}}`. The same execution policy, revision checks, journaling, and output capture apply. The `name` can also be `session` or `shell` with that tool's arguments; supply a distinct ID for each action.

OpenAI compatibility covers streamed Chat Completions text/function tools, not Responses, images, or every server option. Anthropic preserves native response blocks, including supported thinking continuation. The config inspector reports separate capabilities and supported options; broad model-specific support is unprobed. Missing usage and pricing remain unknown. Model IDs must be explicitly configured.

The first live Anthropic check passed on 2026-10-25 with claude-haiku-5-5 selected from the authenticated model list: two streaming requests, one real printf invocation, preserved signed continuation, exact journal-to-context tool results, and read-only replay. See the [live verification report](docs/live-anthropic.md) for evidence, the isolated local configuration, and reproduction commands. This is separate from the fixture suite and does not establish every model's capabilities.

## Execution policy and sessions

Default admission permits executable strings `echo`, `pwd`, `whoami`, and `friendo-fixture`; availability depends on the OS. An absolute path requires that exact path in `execution.allowed_executables`. Arguments are passed as an array. `friendo exec echo -- "hello friendo"` demonstrates argument syntax on hosts with an `echo` executable.

Shell evaluation is a separate model tool and requires both `execution.allow_shell = true` and admission for the explicitly named shell. `allowed_executables = ["*"]` opts into broad admission. This policy is **not an OS sandbox**: admitted programs retain their ordinary host permissions, and arbitrary task constraints are not automatically provable. YOLO keeps the same admission policy and retained explicit constraints.

The `session` model tool supports `start`, `read`, `write`, `interrupt`, `wait`, and `terminate`. Sessions use native pipes, survive individual model turns in the same application, and have the configured execution timeout (30 seconds by default). They do not survive application exit or provide terminal emulation. To exercise a line-oriented session, make `friendo-fixture` discoverable on `PATH` or admit its exact built path, then ask the model to start it with `args = ["session"]`, read, write `hello\n`, read the reply, and terminate it. Every action and result is recorded.

**Ctrl-C** requests cancellation of the current turn and interruption of all retained sessions. A persistent process also has targeted `session interrupt`/`terminate` operations. **q** quits, cancels active work, and terminates retained sessions. Requested cancellation and observed termination are different records; inspect incomplete outcomes. Shell conventions are handled explicitly for Unix shells, `cmd`, and PowerShell.

## Terminal controls

| Control | Action |
| --- | --- |
| `1`–`5` | Topology, Intent, Evidence, Context, Config |
| `j` / `k`, `Enter` | Select a node; inspect provenance/detail |
| Arrow keys, `+` / `-`, `0` | Pan, zoom, reset camera |
| `f`, `a` | Focus selected node; focus latest work |
| `v`, `c` | Toggle list alternative; collapse process/request groups |
| `/`, `.` | Cycle node and edge filters |
| `t`, `d` | Trace upstream sources; highlight downstream impact |
| `p` | Pin artifact contents or selected source detail into subsequent requests |
| `r`, `y`, `i` | Rewire, toggle YOLO, compose |
| `[`, `]`, `Esc` | Previous/next recorded position; leave review/overlay |
| `PageUp` / `PageDown` | Scroll inspectors |
| `?`, `Ctrl-C`, `q` | Help, interrupt, quit |

Effects support `off`, `reduced`, `full` (default), and `overdrive`: `friendo --effects off --no-color demo`. The configured frame budget defaults to 30 FPS; no benchmark claim is made. Windows narrower than 90 columns use the list alternative, and the footer/help show active bindings. `friendo.toml` exposes system prompt, provider options, turn/tool limits, layout, effects/color/FPS, and remaps for repair/YOLO/trace/pin. CLI values override file values, then defaults. File changes apply to the next run; direct YOLO changes apply at the next dispatch. Full keymap, workflow, and automated context-rule editors are not implemented.

## Records, privacy, and review

Runs live in `.friendo/runs/<UUID>/` by default; `--data-dir <path>` selects another root. `events.jsonl` is append-only; `artifacts/` holds requests, responses, and process output. Review reconstructs recorded state and topology without creating an executor. There is no disk resume, rerun, fork, external rollback, or exactly-once side-effect guarantee.

Known configured credential values are redacted before journal/artifact retention and process spooling, including split output reads and escaped/nested JSON strings. Credential references are retained; arbitrary sensitive text is not automatically detected. Other user text, commands, context, and outputs are stored locally until you remove the run. There is no automatic expiry. Child processes inherit the host environment after configured provider credential names are removed; `env_refs` cannot pass those credential references into tools. Other environment values remain available, and execution admission is not an OS sandbox.

Each outbound request body is retained after credential redaction. Context view renders that body; **Enter** opens it inline. Tool excerpts are sanitized and bounded; artifact records expose transformations and capture truncation. Artifact pins read actual retained source contents; other pins include the selected label/detail. Pins over 64 KiB or unavailable sources fail visibly. Expand adds marked 16 KiB excerpts from up to eight retained sources. Provider-internal context is unavailable. Events have 1 MiB record, 64 MiB retained serialized-event, and 100,000-event limits. A truncated final journal line preserves earlier valid events and reports the ignored tail; an action or persistent session lacking completion evidence remains **outcome unknown** and is never automatically replayed or reattached.

## Development and scope

Run the local preflight before pushing:

```console
bash scripts/check.sh
```

It runs formatting, Clippy, all target tests, doctests, and the binary build. On Unix it also runs the event-driven PTY demo and a credential-free live-mode journey against a local HTTP fixture. The fixture invokes one real harmless command, checks exact context/results, and exercises correction and Ctrl-C separately. It uses temporary config/data and never contacts your configured hosted provider. Python is needed only for this Unix QA step.

Ordinary `cargo build` does not execute tests. CI runs Linux preflight before native macOS/Windows checks, retains platform-specific Clippy, and captures failure logs. PRs and pushes to `main` trigger CI; feature branches are checked through their PR, avoiding duplicate branch-push runs. Superseded revisions are canceled. The manual coverage workflow includes the native terminal journeys without adding an instrumented build to every PR. See [testing and coverage](docs/testing.md) for commands, evidence, and remaining platform limits.

Native runtime validation to date is macOS only. Windows/Linux CI configuration is not a passed native runtime test. Deterministic provider fixtures and the synthetic demo are separate from live provider checks. Current omissions include PTY emulation, network discovery, semantic conflict detection, arbitrary constraint enforcement, comprehensive tool-version probing, and distributed execution.

MCW means **Meta-Context Window**. friendo records coordination observations; it does not implement or store MCW, read private human understanding, or calculate a universal alignment score. See [sources and canonical mappings](docs/sources.md), [architecture](docs/architecture.md), [Graffy assessment](docs/graffy-assessment.md), [product specification](docs/product.md), and [build status](docs/build-status.md). GPL-3.0-only; see [LICENSE](LICENSE).
