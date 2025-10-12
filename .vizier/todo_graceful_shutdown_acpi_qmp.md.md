
---
Evidence update (2025-10-12):
- Shutdown path is TERM→10s wait→KILL with no ACPI/QMP (src/core/runtime.rs:638; broker mirrors at :750). Launch plumbing centralizes prep, so inserting a cooperative phase is feasible. This anchors Thread 2.

Acceptance refinement:
- Attempt ACPI/QMP/guest-agent powerdown first; emit Event::Message describing the phase; only then fall back to signals with clear timeouts.


---

