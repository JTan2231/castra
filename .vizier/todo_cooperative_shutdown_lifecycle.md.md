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
Expose configurable shutdown timeouts and render cooperative outcomes consistently.
Describe and expose per‑VM cooperative/TERM/KILL timeouts via CLI/options; render method + timeout/reason fields consistently in CLI and JSON with brief remediation hints for ChannelUnavailable/ChannelError; document example event payloads and sequencing, including the 0ms Unavailable path. (thread: cooperative-shutdown-lifecycle)

Acceptance Criteria:
- CLI exposes options to set cooperative, TERM, and KILL timeouts with clear defaults and help text; values propagate to runtime behavior.
- Event order per VM remains: ShutdownRequested → CooperativeAttempted(method, timeout_ms) → CooperativeSucceeded | CooperativeTimedOut(timeout_ms, reason, detail?) → Escalation(SIGTERM)? → Escalation(SIGKILL)? → ShutdownComplete(outcome, total_ms).
- When the cooperative channel is unavailable, CLI/JSON show CooperativeAttempted(method: Unavailable, timeout_ms=0) immediately followed by CooperativeTimedOut(reason: ChannelUnavailable, detail), with no cooperative wait.
- Channel errors emit CooperativeTimedOut(reason: ChannelError) with a surfaced diagnostic; CLI includes a short remediation hint for ChannelUnavailable/ChannelError.
- CLI (down) remains responsive during waits; per‑VM isolation preserved; behavior is idempotent; JSON fields are stable and documented (method, timeout_ms, reason, detail, total_ms).
- Documentation includes snapshot/example payloads and sequencing, highlighting the 0ms Unavailable path and typical remediation notes.

Pointers:
- src/core/options.rs (timeout surfaces)
- src/core/runtime.rs; src/core/events.rs; src/core/reporter.rs (event emission/fields)
- src/app/down.rs (CLI rendering)
- docs/ (examples and sequencing)