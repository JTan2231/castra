# Thread 2 — QEMU backend and VM lifecycle
Snapshot: v0.7.7 (Current)

Goal
- Add a guest-cooperative shutdown path before falling back to TERM→KILL in `castra down`, with observable, ordered Events.

Why (tension)
- Current shutdown is TERM→KILL only; users expect ACPI/QMP-assisted shutdown for clean FS and faster restarts.

Desired behavior (product level)
- `castra down` attempts an ACPI/QMP powerdown first with a bounded wait; if the VM exits cleanly, no signals are sent. If not, proceed to TERM, then KILL after timeouts.
- Emit ordered Events: `ShutdownInitiated(Graceful)`, `ShutdownEscalation(SIGTERM|SIGKILL)` as taken, and `ShutdownComplete(Graceful|Forced)`; surface in logs and OperationOutput.
- Idempotent and safe if invoked repeatedly; respects global exit code policy.
- Per-VM behavior; one stuck guest doesn’t block others from stopping.

Acceptance criteria
- On ACPI-honoring guests, `castra down` completes without TERM/KILL and status becomes stopped; logs show the graceful path and Events sequence.
- On unresponsive guests, escalation mirrors today’s behavior with visible, bounded timeouts; Events reflect the escalations.
- Events appear in OperationOutput and are streamable to UIs.

Scope and anchors (non-prescriptive)
- Anchors: src/core/runtime.rs (lifecycle); src/core/events.rs; src/core/options.rs (timeouts/config); src/app/down.rs (user-facing copy).
- Keep mechanism open (QMP system_powerdown vs ACPI inject), but ensure Events are emitted in order and surfaced in OperationOutput.
