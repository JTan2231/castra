---
Thread 2 — Cooperative shutdown lifecycle (canonical)

Tension
- We promise predictable, observable graceful shutdown; previously only TERM→KILL. Sequencing is shipped; now expose configurability and polish surfacing.

Change (product level)
- Expose per‑VM cooperative/TERM/KILL timeouts via CLI/options; render method + timeout/reason consistently in CLI and JSON; include brief remediation hints for ChannelUnavailable/ChannelError. Document example event payloads and sequencing, including the 0ms Unavailable path.

Acceptance Criteria
- CLI exposes cooperative, TERM, and KILL timeouts with clear defaults/help; values propagate to runtime behavior.
- Event order per VM remains: ShutdownRequested → CooperativeAttempted(method, timeout_ms) → CooperativeSucceeded | CooperativeTimedOut(timeout_ms, reason, detail?) → Escalation(SIGTERM)? → Escalation(SIGKILL)? → ShutdownComplete(outcome, total_ms).
- Unavailable path: CooperativeAttempted(method: Unavailable, timeout_ms=0) immediately followed by CooperativeTimedOut(reason: ChannelUnavailable, detail) with no wait.
- Channel errors emit CooperativeTimedOut(reason: ChannelError) with surfaced diagnostics and a short hint.
- `castra down` remains responsive during waits; per‑VM isolation preserved; behavior idempotent; JSON fields stable and documented.

Pointers
- src/core/options.rs; src/core/runtime.rs; src/core/events.rs; src/core/reporter.rs; src/app/down.rs; docs/ (examples and sequencing)
---Thread link: Thread 40 — Stabilization and polish. Context: Harness now owns SSH sessions to in-VM Vizier (vizier.remote.* live). Acceptance: graceful stop sequence propagates from UI→harness→VM Vizier with bounded timeouts; reconcilers do not respawn during intentional shutdown; status reflects draining state; logs clearly mark shutdown boundaries.

---

