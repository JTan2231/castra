Status — CLOSED in v0.7.2
- Evidence: README.md now declares MSRV (Rust 1.77) with install guidance; docs/RELEASING.md added with a reproducible release checklist; build.rs and src/cli.rs wire version + short SHA as implemented.
- Acceptance met: CLI version output shows `<semver> (<short SHA>)` when VCS metadata exists; README surfaces MSRV; RELEASING.md exists with steps.
- Follow-up: Treat MSRV upkeep as hygiene, not an open thread.


---

