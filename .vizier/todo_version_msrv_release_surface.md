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
Snapshot reference bumped to v0.7.1. Acceptance unchanged; note that README top-level may surface MSRV while docs/RELEASING.md carries process detail.

---

Refinement (anchors + clarity)
- Anchors: src/cli.rs (version string path via clap/command), Cargo.toml (no build.rs present today). Note: build mechanism is currently absent; embedding SHA likely requires build.rs or env var plumbing.
- Acceptance nuance: when built without VCS metadata (e.g., crates.io tarball), `castra --version` must omit SHA cleanly while preserving a stable format (`<semver>` or `<semver> (<sha>)`).
- Docs anchors: README (MSRV badge/section) and docs/RELEASING.md (new).

---

Status update — build metadata now embedded
- Evidence: build.rs present and sets CASTRA_VERSION to "<semver> (<short SHA>)" when git is available; src/cli.rs consumes env!("CASTRA_VERSION"). Test `command_reports_embedded_version_string` asserts version wiring.

Impact
- The version-string half of this thread is satisfied. `castra --version` prints the semver and short SHA in git checkouts; falls back to just semver when SHA is unavailable.

Remaining scope (narrowed)
- Declare and surface MSRV in README.md (badge/section).
- Author a lightweight release process in docs/RELEASING.md (tags, CHANGELOG cadence, binary packaging notes). Keep tool choices open.

Acceptance (updated)
- README contains an explicit MSRV statement.
- docs/RELEASING.md exists with a minimal, reproducible set of steps to cut a release.
- No changes needed to CLI version output unless we choose to also expose full SHA via `--verbose` or similar (out of scope for now).

Anchors
- Docs: README.md, docs/RELEASING.md.
- Version path: build.rs, src/cli.rs (complete).


---

