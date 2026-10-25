# Live Anthropic verification — 2026-10-25

**Passed on macOS/aarch64.** The first live attempt completed without an application fix. This evidence is separate from the deterministic provider fixtures.

## Model selection and bounds

The existing ANTHROPIC_API_KEY environment reference authenticated GET /v1/models successfully (HTTP 200). The returned catalog identified claude-haiku-5-5 as active in the Haiku line; it was selected from that response rather than guessed. The non-secret catalog is retained locally at .friendo/live-verification/anthropic-models-2026-10-25.json.

The isolated configuration at .friendo/live-verification/anthropic.toml sets:

- Native Anthropic Messages endpoint https://api.anthropic.com/v1.
- Model claude-haiku-5-5, max_tokens 1024, and at most three model requests.
- NORMAL autonomy; effects off.
- Only /usr/bin/printf admitted, shell disabled, five-second process deadline, and 64KiB retention per output stream.
- Credential reference ANTHROPIC_API_KEY; no credential value in the file.

The task requested exactly one direct invocation with args ["FRIENDO_ANTHROPIC_LIVE_OK\n"]. User constraints prohibited extra commands, shell/session use, and user-file changes; friendo's own journal/artifact persistence was expected.

## Observed full loop

Run ID: **47c58f96-42dc-4ece-857a-2e50314e3d3c**.

1. friendo sent a streaming Messages request with the exact task, explicit constraint, and native tool schemas.
2. Anthropic returned native thinking and tool_use blocks. The thinking block had a signature.
3. friendo dispatched one exec invocation: executable /usr/bin/printf; args array containing the marker followed by a newline.
4. The process exited 0, captured 26 stdout bytes and zero stderr bytes, and reported output_complete true with no truncation or capture errors.
5. The second outbound request preserved the first native content array, including its signed block, and returned the exact journaled process result under the matching tool_use_id.
6. Anthropic returned a final text answer reporting the observed marker, exit code, and capture status. The CLI exited 0, with no stderr. Total observed elapsed time was 3.252 seconds for this single run; this is not a performance benchmark.

Exact stdout:

    FRIENDO_ANTHROPIC_LIVE_OK

The marker is followed by one LF byte. The machine-readable audit records the exact string.

Provider-reported usage was 1,293 input / 146 output tokens for the tool request and 1,843 input / 215 output tokens for the final answer: **3,136 input / 361 output total**. Output usage includes 42 reported thinking tokens in the first response. Price was not calculated; friendo continues to report unknown cost.

## Evidence inspection

The retained [audit export](verification/anthropic-live-2026-10-25.json) records paths, IDs, counts, usage, scope, and the journal SHA-256. Raw run artifacts remain local and ignored by Git:

- [Journal](../.friendo/runs/47c58f96-42dc-4ece-857a-2e50314e3d3c/events.jsonl): 56 valid schema-1 events, consistent run/branch identity, unique IDs, and resolved causal references. No failure event.
- [First request](../.friendo/runs/47c58f96-42dc-4ece-857a-2e50314e3d3c/artifacts/9e98c6a8-9a07-4814-a770-1a6ef6e68371.txt): exact task, constraints, revision, selected model, token bound, and stream true.
- [Second request](../.friendo/runs/47c58f96-42dc-4ece-857a-2e50314e3d3c/artifacts/8e561ce4-ce9e-463e-9a0e-bb7cb6f21901.txt): native assistant content matches the first response continuation; tool_result content parses to the exact ToolFinished result, preserving the native call ID.
- [Final response](../.friendo/runs/47c58f96-42dc-4ece-857a-2e50314e3d3c/artifacts/3019d365-72fc-460f-b479-98c715b3c29d.txt): retained text answer and provider-reported usage.
- The stdout artifact equals the expected marker bytes; the stderr artifact is empty.

Headless replay, context replay, and review at the tool-start position succeeded. SHA-256 hashes of every run file were identical before and after these read-only commands. The configured credential value and its JSON-escaped representation were absent from the 13 retained live-run/verification files scanned. Authentication headers are excluded from outbound body artifacts. This is a literal known-credential scan, not a claim of universal sensitive-data detection.

No LINK RESTORED event was invented for the live smoke. The recorded mechanical check concerns current revision delivery/interpretation, not semantic correctness or shared understanding. Descendant status reported process-group absence with escaped descendants untracked.

## Reproduce or inspect

Build with cargo build --locked --bins. The retained configuration and goal can be exercised again with:

    ./target/debug/friendo --config .friendo/live-verification/anthropic.toml run --headless 'Call exec exactly once with executable "/usr/bin/printf" and args ["FRIENDO_ANTHROPIC_LIVE_OK\n"], then report the observed stdout, exit code, and output_complete.'

Read the existing run without another model call:

    ./target/debug/friendo replay 47c58f96-42dc-4ece-857a-2e50314e3d3c --headless
    ./target/debug/friendo replay 47c58f96-42dc-4ece-857a-2e50314e3d3c --context

The verification configuration is intentionally isolated; no default friendo.toml, account setting, credential, or global execution policy was changed. The .friendo files exist in this workspace and are not committed. A fresh clone can recreate the config from the limits above using its own environment credential.

## Regression checks and limits

cargo build --locked --bins, cargo fmt --all --check, cargo clippy --all-targets --locked -- -D warnings, and cargo test --all-targets --locked passed. The suite still has 46 passing tests: 13 unit, 18 process, 10 provider, and 5 integrated runtime.

This run verifies one available model's native streaming text/tool round trip, including observed signed continuation. It does not verify every model, every explicit thinking option, PTY interaction, Windows/Linux runtime behavior, or broad production readiness. No additional live inference was required.

Primary protocol references consulted: [Models API](https://platform.claude.com/docs/en/api/models/list), [Messages API](https://platform.claude.com/docs/en/api/messages/create), and [streaming event flow](https://platform.claude.com/docs/en/build-with-claude/streaming). The successful live calls used the adapter's existing anthropic-version 2023-06-01 header; no protocol-version change was needed.
