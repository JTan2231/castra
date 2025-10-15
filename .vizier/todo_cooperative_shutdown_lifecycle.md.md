---
Thread 2 — Cooperative shutdown lifecycle (canonical)

Tension
- Current behavior falls back to TERM→KILL only; we promise predictable, observable graceful shutdown.

Change (product-level)
- Add a guest-cooperative shutdown attempt before signals with bounded waits and ordered, machine-parseable events per-VM.

Event sequence (stable names/fields)
1) ShutdownRequested { vm_id }
2) CooperativeAttempted { vm_id, method: ACPI | QMP | Agent }
3) CooperativeSucceeded { vm_id, duration_ms } | CooperativeTimedOut { vm_id, timeout_ms }
4) Escalation { vm_id, signal: SIGTERM, wait_ms }? (only if step 3 timed out)
5) Escalation { vm_id, signal: SIGKILL, wait_ms }? (only if TERM timed out)
6) ShutdownComplete { vm_id, outcome: Graceful | Forced, total_ms }

Acceptance criteria
- On ACPI/QMP-capable guests, `castra down` completes via CooperativeSucceeded without TERM/KILL; status shows stopped; exit code signals success.
- On unresponsive guests, escalations occur with visible, bounded waits; events/logs show the exact path; command returns success when all targeted VMs are stopped.
- Per-VM isolation: multiple VMs can shut down concurrently; one stuck VM does not block others, and each emits its own ordered sequence.
- Configurable timeouts via CLI/options; sane defaults; behavior is idempotent.
- Events appear in logs and JSON output with stable field names.

Pointers (non-prescriptive anchors)
- src/core/runtime.rs (shutdown orchestration)
- src/core/events.rs (event variants)
- src/core/options.rs (timeouts/config)
- src/app/down.rs (user-facing surfaces)
- src/core/reporter.rs (event emission)

Notes
- Mechanism selection remains open (ACPI/QMP/Agent) so long as observable sequence and bounds are honored.
---
