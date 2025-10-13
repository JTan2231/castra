Progress note — Snapshot v0.7.6

- Partial delivery: Configurable lifecycle timeouts are implemented and threaded through shutdown (`graceful/term/kill` waits). CLI help and config parsing in place.
- Evidence: commit 2353031 (feat: configurable shutdown timeouts via lifecycle). Anchors touched: src/core/options.rs; src/app/down.rs; src/core/runtime.rs.

Remaining scope
- Implement the guest-cooperative phase (ACPI/QMP system_powerdown or equivalent) before TERM→KILL.
- Emit ordered Events: ShutdownInitiated(Graceful) → ShutdownEscalation(SIGTERM|SIGKILL) → ShutdownComplete(Graceful|Forced) in logs and OperationOutput.
- Per-VM independence and bounded waits remain acceptance criteria.

Acceptance refinement
- With lifecycle waits configured, successful graceful path should complete within `graceful_wait`; failing that, escalations occur precisely at configured boundaries and are visible via Events/logs.


---

