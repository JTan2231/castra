---
Update (Snapshot v0.8.0 → align events + scope)

Why
- Harmonize lifecycle event naming with Snapshot Thread 2 and keep product-level surfaces consistent.

Desired behavior (product level)
- `castra down` performs a guest-cooperative shutdown attempt first, then escalates to signals if needed. Observable, ordered events per-VM:
  1) ShutdownRequested
  2) CooperativeAttempted (method: ACPI|QMP|Agent)
  3) CooperativeSucceeded | CooperativeTimedOut
  4) Escalation(SIGTERM) [optional]
  5) Escalation(SIGKILL) [optional]
  6) ShutdownComplete(Graceful|Forced)
- Timeouts are configurable via existing options; command remains responsive and per-VM isolation is preserved.

Acceptance criteria
- On ACPI/QMP-honoring guests, shutdown completes at (6) with Graceful and no Escalation events.
- On unresponsive guests, bounded waits produce CooperativeTimedOut then Escalation steps; exit when processes are gone; outcome Forced.
- Events appear in per-VM logs and OperationOutput/JSON with stable names/fields and timestamps.

Anchors
- src/core/runtime.rs (orchestration)
- src/core/events.rs (event variants surface)
- src/core/options.rs (timeouts)
- src/app/down.rs (user-facing output)

Notes
- Keep mechanism choice open (ACPI/QMP/agent) while guaranteeing the observable sequence above. Idempotent if invoked repeatedly.

---

