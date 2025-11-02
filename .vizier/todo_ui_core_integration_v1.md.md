---
Pivot notice — superseded by Harness ↔ Core (vizier-first)
Status:
- This thread is superseded by Thread 20 (Harness ↔ Core integration). UI should subscribe to the harness unified event stream rather than binding directly to castra-core.
Reframed scope (product-level):
- UI consumes the harness unified stream and renders Up live status; no direct core embedding.
- Acceptance alignment: UI must handle version preamble + vizier operational context preamble before regular events.
Anchors:
- Subscribe via castra-harness stream surfaces; maintain component mappings in castra-ui as-is.
Next action:
- See `todo_harness_vizier_ssh_first_integration.md` and `todo_ui_vertical_slice_up_status.md` for active work.

---

