Narrowing — docs follow-up added (v0.7.2)
- Remaining action: Update AGENTS.md with a minimal embedding example and explicit feature guidance (disable default features, enable only what’s needed). Link to docs/library_usage.md.
- Acceptance unchanged: AGENTS.md must include a snippet and call out `default-features = false`.
- Evidence anchor: Current AGENTS.md is a placeholder and lacks embedder guidance.


---

Narrowed scope — docs only (v0.7.2)
- Status: Feature gating implemented and verified; remaining work is documentation for embedders.

What remains
- Update AGENTS.md with a minimal embedding snippet demonstrating use with `default-features = false` and calling into core APIs.
- Cross-link to docs/library_usage.md feature policy and note MSRV.

Acceptance (docs-focused)
- AGENTS.md includes:
  - A short code example showing initialization and invoking a core operation without the CLI feature.
  - A note on adding `castra = { version = "x.y", default-features = false }` in Cargo.toml.
  - A brief explanation of available surfaces (core::{operations, options, outcomes, events}).
- CI/docs build passes; no code changes required.

---

