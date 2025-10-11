Thread: Toolchain baseline and distribution (depends on SNAPSHOT v0.1)

Goal
- Establish reliable builds and version surfacing.

Acceptance criteria
- `castra --version` shows semver and git short SHA when available.
- Document minimum supported Rust (MSRV) and pin in CI; local builds work via `cargo install --path .`.
- Release artifacts plan captured (cargo-dist or equivalent not mandated) with a simple checklist in RELEASING.md.

Notes
- Avoid hard-coding library choices; focus on observable outputs and docs.