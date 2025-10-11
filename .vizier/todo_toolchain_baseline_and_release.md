Thread: Toolchain baseline and distribution (depends on SNAPSHOT v0.1)

Goal
- Establish reliable builds and version surfacing.

Acceptance criteria
- `castra --version` shows semver and git short SHA when available.
- Document minimum supported Rust (MSRV) and pin in CI; local builds work via `cargo install --path .`.
- Release artifacts plan captured (cargo-dist or equivalent not mandated) with a simple checklist in RELEASING.md.

Notes
- Avoid hard-coding library choices; focus on observable outputs and docs.---
Update (SNAPSHOT v0.2)

Evidence
- `--version` is wired via clap (uses Cargo package version). Git SHA not yet surfaced.
- No MSRV pinned; no release checklist/docs present.

Refinement
- Surface git short SHA in --version when built from a git checkout (optional when unavailable).
- Document MSRV and add a note in README/RELEASING.md; keep packaging tool choice open.

Acceptance criteria (amended)
- `castra --version` shows `0.x.y (git <shortsha>)` when GIT info available; otherwise just semver.

---

