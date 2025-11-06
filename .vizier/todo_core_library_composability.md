# castra-core library composability surface

Goal: Ensure castra-core can be embedded and used without the UI, emitting events to an external subscriber.

Acceptance criteria:
- Public API to run Up/Down/Status/Clean/Bootstrap programmatically with a caller-provided sink/handler for events.
- Library usage documented with minimal examples (Rust), plus guidance for process-boundary JSON for non-Rust consumers.
- No UI-specific dependencies leak into the core crate; UI remains an optional, separate binary.

Scope:
- Keep implementation open; preserve current streaming/event model.
- Provide examples under castra-core/examples/ showing event subscription and operation invocation.

Anchors: castra-core/src/lib.rs; castra-core/src/core/runtime.rs; castra-core/src/core/events.rs; castra-core/src/app/*; examples/.

Threads: Thread 20; supports Threads 2, 12, 13.Pivot alignment (Vizier removal):
- Update thread references: serves Thread 40 (Post-removal stabilization) and Thread 50 (Vizier Removal), unblocks Thread 31 (Agent-first). Remove Thread 20/12/13 legacy mentions.
- Clarify that examples demonstrate emitting core events independent of any vizier.remote protocol; consumers can bridge to UI-managed agent sessions as needed.

Anchors unchanged. Acceptance unchanged.

---

