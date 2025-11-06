Thread 50 — Vizier Removal — Workstream 5: Documentation & institutional memory

Tension
- Public docs and READMEs still describe Vizier-era flows.

Desired behavior (product level)
- Internal docs reflect the new architecture (core boots VMs; UI owns agent sessions; harness provides metadata). Legacy Vizier references are clearly marked or removed.

Acceptance criteria
- README.md, docs/BOOTSTRAP.md, castra-ui and castra-harness docs updated; no stale links.
- vizier-removal folder includes this plan and a transient checklist; any temporary cfg/flags tracked and then removed before closing.

Pointers
- README.md; castra-core/docs/*; castra-ui/docs/*; castra-harness/README.md; vizier-removal/*