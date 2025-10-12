Snapshot alignment update (2025-10-12):
- Snapshot reference bumped to v0.7.1. Emphasize emitting explicit Events during staged shutdown per SNAPSHOT direction.

Additional acceptance note:
- During graceful path, emit ordered Events: `ShutdownInitiated(Graceful)`, `ShutdownEscalation(SIGTERM|SIGKILL)` as applicable, and `ShutdownComplete(Graceful|Forced)`; surface in logs and OperationOutput.


---

