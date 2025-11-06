Thread 50 — Vizier Removal — Workstream 1: Core bootstrap refactor

Tension
- Current core bootstrap stages and reports on in-VM Vizier, violating the pivoted institutional boundary (core must not manage long-lived guest services).

Desired behavior (product level)
- `castra-core` prepares VMs and reports bootstrap outcomes without any Vizier-specific statuses, environment, or remediation hints.
- CLI surfaces only VM/bootstrap artifacts. No references to `castra-vizier` or vizier logs.

Acceptance criteria
- Workspace builds without `castra-vizier` as a member; crate removed.
- Bootstrap plan/run outcomes compile and execute with Vizier fields removed; event ordering remains coherent.
- `castra-core` docs and help text contain no Vizier references (except clearly marked legacy notes).

Pointers
- castra-vizier/ (delete)
- castra-core/src/core/bootstrap.rs (strip Vizier staging/units)
- castra-core/src/app/up.rs (simplify output)
- castra-core/src/core/events.rs, options.rs (remove fields/flags)
- Root Cargo.toml membership

Notes
- Keep implementation open; sequence deletions to avoid transient compile breaks; if temporary cfg stubs are introduced, track them in vizier-removal transient checklist.Status update (v0.12.2)
- Outcome: LANDED. Core no longer stages/installs/probes Vizier; events/steps/plan fields removed; CLI/app output scrubbed; docs updated.
- Evidence: git diff drops Vizier enums/steps (BootstrapStepKind::*Vizier*), removes VizierPlanStatus and broker events, simplifies operations::up, and updates BOOTSTRAP.md/ARCHITECTURE_HIGH_LEVEL.md/WORKSPACE.md. examples/library_up.rs now calls operations::up with no launcher.
- Acceptance: Met. Leave thread open for one release to watch for regressions; then mark CLOSED.

---

