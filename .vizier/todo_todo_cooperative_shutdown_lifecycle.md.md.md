
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

Expose configurable shutdown timeouts and render cooperative shutdown outcomes consistently.
Describe and expose per-VM cooperative/TERM/KILL timeouts via CLI/options; render method + timeout/reason fields in CLI and JSON with brief remediation hints for ChannelUnavailable/ChannelError; document example event payloads and sequencing, including the 0ms Unavailable path. (thread: cooperative-shutdown-lifecycle)

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