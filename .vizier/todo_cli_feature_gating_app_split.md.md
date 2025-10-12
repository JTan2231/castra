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

Scope tightened to docs-only follow-up.

- Context (Thread 11: Library API stability for embedders; Snapshot v0.7.2): Code gating is DONE; remaining work is documentation alignment.

Acceptance criteria
- AGENTS.md: add a minimal embedding example showing how to depend on castra as a library with default-features = false and feature = ["cli"] when binary is desired. Cross-link to docs/library_usage.md.
- README.md: brief note pointing embedders to AGENTS.md and library_usage.md. Reiterate MSRV statement (Rust 1.77) as informational, not a task.
- No code changes required; this TODO closes when docs PR merges.

Anchors
- docs/library_usage.md (existing guidance)
- AGENTS.md (to be updated)
- README.md (brief pointer)

Notes
- Do not prescribe crate/module layouts; keep examples minimal and feature-focused.

---

