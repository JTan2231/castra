

---
Progress update (v0.8.6)
- `down` now performs per-VM concurrent shutdown with live event streaming.
- Runtime emits CooperativeAttempted with timeout_ms=0 when the cooperative channel is unavailable; CooperativeTimedOut includes reason=ChannelUnavailable on that path.
- CLI treats forced shutdowns as success while listing forced VMs as a warning.
- Tests cover QMP success/timeout and unavailable-channel semantics (ordered events, 0ms wait, escalation path).

Next slice
- Emit CooperativeAttempted and CooperativeSucceeded/CooperativeTimedOut prior to TERM/KILL for available channels, honoring configurable timeouts and per-VM isolation.
- Maintain ordered events and stable JSON fields; ensure idempotence across retries.
Acceptance addendum
- Event order must be: ShutdownRequested → CooperativeAttempted(method, timeout_ms?) → CooperativeSucceeded | CooperativeTimedOut(timeout_ms, reason?) → Escalation(SIGTERM)? → Escalation(SIGKILL)? → ShutdownComplete(outcome, total_ms).
---

---

