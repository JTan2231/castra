---
Consolidation note (Snapshot v0.8.1)

- This item is folded into Thread 2 canonical: "cooperative_shutdown_lifecycle.md".
- Keep anchors for orientation (src/core/runtime.rs; src/core/events.rs; src/core/options.rs; src/app/down.rs), but defer event naming and acceptance to the canonical TODO.
- Event sequence to follow Snapshot Thread 2: ShutdownRequested → CooperativeAttempted(method: ACPI|QMP|Agent) → CooperativeSucceeded | CooperativeTimedOut → Escalation(SIGTERM)? → Escalation(SIGKILL)? → ShutdownComplete(outcome: Graceful|Forced).


---

