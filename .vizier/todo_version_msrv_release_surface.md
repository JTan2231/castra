# Thread 9 — Toolchain baseline and distribution
Snapshot: v0.7 (Current)

Goal
- Enrich version/distribution surface: include git SHA in `castra --version`; document MSRV and basic release workflow.

Why (tension)
- Snapshot Thread 9: version shows Cargo semver only; MSRV/release docs are TBD.

Desired behavior (product level)
- `castra --version` prints `<semver> (<short SHA>)` when built from a git checkout; falls back gracefully when info is unavailable.
- Repository docs declare the MSRV and a brief release procedure (tagging, binaries, CHANGELOG cadence).

Acceptance criteria
- Running `castra --version` in a dev build shows a short SHA; in release tarballs/binaries the SHA is present if embedded or omitted with clear format.
- A docs page (README or docs/RELEASING.md) lists MSRV and steps for cutting a release; CI or scripts are optional.

Scope and anchors (non-prescriptive)
- Anchors: src/cli.rs (version string), Cargo build scripts if used; docs/.* for MSRV/releasing.
- Keep build mechanism open (env vars, build.rs); avoid over-prescribing tooling.
