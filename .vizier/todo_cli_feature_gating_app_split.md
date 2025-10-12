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
