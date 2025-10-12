Thread link: Thread 11 (Library API stability for embedders)

Tension
- Disabling the `cli` feature still compiles `pub mod app`, which depends on `crate::cli`, breaking intended feature gating for embedders.

Evidence
- lib.rs exposes `app` unconditionally; `app` uses CLI-specific helpers, causing build failures when the `cli` feature is off.

Change (product-level)
- Ensure embedders can depend on castra without pulling in clap/CLI presentation code. Gate `app` behind the `cli` feature or move presentation-only helpers behind that gate.

Acceptance criteria
- Building the crate with `--no-default-features` succeeds on stable Rust and exposes only the library API (core::{operations, options, outcomes, events}).
- `cargo features` shows `cli` gating app and clap dependencies.
- docs/library_usage.md reflects the clarified feature-gating policy and provides examples for embedders.
Snapshot reference bumped to v0.7.1. Keep mechanism open; acceptance unchanged. Note: update docs/library_usage.md and AGENTS.md to reflect feature policy for embedders.

---

Anchors
- src/lib.rs (unconditional `pub mod app`) and src/app/mod.rs (CLI-coupled helpers).
- Cargo.toml features section; clap and presentation-only deps should sit behind `cli`.

Acceptance refinement
- Building with `--no-default-features` removes clap and other CLI-only crates from the dependency graph (verified via `cargo tree -e features`).
- docs/AGENTS.md updated alongside docs/library_usage.md to show embedding without `cli` and minimal feature sets.

---

Status update — feature gating implemented
- Evidence: src/lib.rs gates `cli` and `app` modules behind `#[cfg(feature = "cli")]`; Cargo.toml sets `default = ["cli"]`, marks `clap` as optional, and requires `cli` feature for the `castra` bin; docs/library_usage.md instructs embedders to use `default-features = false`.
- Verification: Building with `--no-default-features` compiles the library surfaces only (core, config, error, managed) and excludes the binary and clap.

Impact
- Primary tension resolved: embedders can depend on castra without CLI surface/deps.

Remaining scope (narrowed)
- Update AGENTS.md to reflect the feature policy and provide a minimal embedding example.

Acceptance (updated)
- AGENTS.md contains an embedding snippet and explicitly calls out disabling default features.

Anchors
- src/lib.rs, Cargo.toml, docs/library_usage.md.


---

