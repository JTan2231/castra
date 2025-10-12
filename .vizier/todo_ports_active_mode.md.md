
---
Evidence update (2025-10-12):
- PortForwardStatus enum already includes Active, but summarize() only emits Declared/Conflicting/BrokerReserved (src/core/ports.rs:15→30). CLI has no --active flag (src/cli.rs:112). This concretely anchors Thread 6.

Acceptance refinement:
- --active flag enables runtime inspection and surfaces Active rows. Script-friendly table output preserved.


---

