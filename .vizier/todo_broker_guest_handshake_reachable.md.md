
---
Evidence update (2025-10-12):
- Status rows cannot show `reachable` today; they hardcode `waiting` when the broker pidfile exists (src/core/status.rs:45) while the broker writes only a static greeting on accept (src/core/broker.rs:64). This confirms the user-facing gap referenced in Thread 3 — no handshake tracking or freshness window exists.

Acceptance refinement:
- Status table BROKER column must transition among {offline, waiting, reachable} based on a real handshake signal within a freshness window, persisted in-process and reflected in OperationOutput<StatusOutcome>.


---

