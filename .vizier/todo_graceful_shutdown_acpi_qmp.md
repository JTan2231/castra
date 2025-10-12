# Thread 2 — QEMU backend and VM lifecycle
Snapshot: v0.7 (Current)

Goal
- Add a guest-cooperative shutdown path before falling back to TERM→KILL in `castra down`.

Why (tension)
- Snapshot Thread 2: current shutdown is TERM→KILL only; users expect ACPI/QMP-assisted shutdown for clean FS and faster restarts.

Desired behavior (product level)
- `castra down` attempts an ACPI/QMP powerdown first with a bounded wait; if the VM exits cleanly, no signals are sent. If not, proceed to TERM, then KILL after timeouts.
- Progress events and logs reflect the path taken per VM (e.g., "sent ACPI shutdown", "escalating to SIGTERM").
- Idempotent and safe if invoked repeatedly; respects global exit code policy.

Acceptance criteria
- On guests that honor ACPI, `castra down` completes without TERM/KILL and status goes to stopped; logs show the graceful path.
- On unresponsive guests, `castra down` escalates as today and exits successfully once processes are gone; timeouts are visible and bounded.
- Behavior is per-VM; one stuck guest doesn’t block others from stopping.

Scope and anchors (non-prescriptive)
- Anchors: src/core/runtime.rs (shutdown path), src/app/down.rs (messages), tests around status transitions.
- Keep mechanism open (QMP/system_powerdown, ACPI inject, or monitor command); choose the lowest-friction path that works across platforms.
# Thread 2 — QEMU backend and VM lifecycle
Snapshot: v0.7.1 (Current)

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
- Anchors: src/core/runtime.rs (shutdown path), src/app/down.rs (messages), src/core/events.rs (Event variants), tests around status transitions.
- Keep mechanism open (QMP system_powerdown, ACPI inject, or monitor command) to fit supported platforms.
Anchors + events clarity
- src/core/events.rs: introduce specific variants `ShutdownInitiated(Graceful)`, `ShutdownEscalation(SIGTERM|SIGKILL)`, `ShutdownComplete(Graceful|Forced)`; wire through reporter/logs.
- Keep mechanism open (QMP/system_powerdown vs ACPI inject), but ensure Events are emitted in order and surfaced in OperationOutput.

---

Thread 2 — QEMU backend and VM lifecycle. Snapshot v0.7.2 reference.

Tension
- Shutdown path uses TERM→wait→KILL only; lacks cooperative phases (ACPI/QMP/guest-agent), risking data loss and poor UX.

Change (product-level)
- Introduce multi-phase graceful shutdown before signals with observable Events and configurable timeouts.
- Event sequence: ShutdownInitiated → ShutdownEscalation (with reason) → ShutdownComplete (success/failure) emitted to logs and OperationOutput.
- Document sane defaults and configuration knobs.

Acceptance criteria
- `castra down` attempts ACPI power button or equivalent, then escalates to QMP/agent if available before signals; timeouts are honored.
- Logs and OperationOutput show the ordered events with timestamps.
- If graceful path is unavailable or times out, signal-based fallback occurs and is explicitly logged.

Anchors
- src/core/runtime.rs (lifecycle); src/core/events.rs; src/core/options.rs (timeouts/config); src/app/down.rs (user-facing copy).