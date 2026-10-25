# friendo development

Read docs/product.md and docs/architecture.md before changing runtime semantics. Keep the application native Rust; Python in tests/terminal_smoke.py is an optional QA helper only.

The append-only journal is authoritative. Rendering, animation, topology edges, and model/tool output cannot create authority, autonomy changes, or successful checks. Preserve exact user statements, source identity, explicit constraints, and uncertainty. MCW is emergent coordination, never stored or implemented by this program.

Keep provider encoding, execution policy, process lifetimes, storage, and presentation separate. Recheck revision/cancellation at dispatch. Never replay an external action automatically after uncertain completion. Any changed journal schema must have an explicit compatibility decision.

Run cargo fmt --all --check, cargo clippy --all-targets --locked -- -D warnings, and cargo test --all-targets --locked. Provider fixtures need loopback networking. Native Unix terminal QA: build bins, then python3 tests/terminal_smoke.py. Record actual platforms and mocked versus live results in docs/build-status.md.

Do not commit generated runs, credentials, or .claude index state. Keep source/output transformations and configured limits observable. Preserve the original LICENSE and document selective third-party reuse.
