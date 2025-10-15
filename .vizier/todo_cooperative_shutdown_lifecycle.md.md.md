
---
Progress update (v0.8.5+)
- `down` executes per-VM shutdown concurrently and streams events live (ensures per-VM isolation and responsiveness).
- Runtime emits CooperativeAttempted with timeout_ms=0 when the cooperative channel is unavailable; corresponding CooperativeTimedOut may include reason=ChannelUnavailable for clarity.
- Tests (unix-gated) cover QMP cooperative success and timeout, plus unavailable-channel semantics (ordered events asserted).

Next slice
- Emit full cooperative attempt/timeout/success sequencing for available channels prior to TERM/KILL, honoring configurable timeouts per VM.
- Preserve ordered per-VM events and stable JSON fields; ensure `total_ms` rendered in ShutdownComplete remains accurate when cooperative path succeeds.

Acceptance refinements
- Event order must be: ShutdownRequested → CooperativeAttempted(method, timeout_ms?) → CooperativeSucceeded | CooperativeTimedOut(timeout_ms, reason?) → Escalation(SIGTERM)? → Escalation(SIGKILL)? → ShutdownComplete(outcome, total_ms).
- When channel is unavailable, CooperativeAttempted emits with timeout_ms=0 and we proceed without delay to escalation; logs show reason.
- Concurrency: stuck VMs do not block others; UI remains responsive with live streaming.
- Configurable timeouts via CLI/options; idempotent on re-run.

Anchors
- src/core/runtime.rs; src/core/events.rs; src/core/options.rs; src/core/reporter.rs; src/app/down.rs.
---


---

