
---
Update — v0.8.6 sequencing shipped

Progress
- Runtime now emits CooperativeAttempted(method, timeout_ms) and CooperativeSucceeded/CooperativeTimedOut prior to TERM/KILL.
- When the cooperative channel is unavailable, we emit CooperativeAttempted(method: Unavailable, timeout_ms=0) immediately followed by CooperativeTimedOut(reason: ChannelUnavailable, detail) — no wait, then escalate.
- When available (e.g., ACPI), graceful attempt either succeeds (CooperativeSucceeded) or times out (CooperativeTimedOut(reason: TimeoutExpired)). Channel errors report CooperativeTimedOut(reason: ChannelError) with a diagnostic. Per‑VM ordering preserved; total_ms reported in ShutdownComplete.
- Tests cover stale/invalid QMP socket paths, error detail propagation, ordered escalations, and zero‑wait semantics.

Remaining scope (next slice)
- Configurability: expose/propagate cooperative, TERM, and KILL timeouts via CLI/options; ensure help text and defaults are clear.
- Surfacing: render method and timeout/reason fields consistently in CLI (down) and JSON logs; add brief remediation hints when ChannelUnavailable/ChannelError occurs.
- Documentation: snapshot/examples of event payloads and sequencing; mention 0ms unavailable path.

Acceptance refinements
- Event order is as specified; for unavailable channels, timeout_ms=0 is observable and no cooperative wait occurs.
- CLI remains responsive; per‑VM isolation maintained; idempotent behavior; stable JSON fields including reason/detail for CooperativeTimedOut.

Anchors
- src/core/runtime.rs; src/core/events.rs; src/core/options.rs; src/core/reporter.rs; src/app/down.rs.
---


---

