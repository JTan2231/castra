

---
Progress update (v0.8.6)
- Parallel per-VM shutdown with live event streaming is shipped.
- When the cooperative channel is unavailable, runtime emits CooperativeAttempted with timeout_ms=0 followed by CooperativeTimedOut(reason: ChannelUnavailable) and immediate escalation; unix-gated tests cover QMP success/timeout and unavailable paths.
- CLI now treats forced shutdowns as overall success (warning lists forced VMs) to align with lifecycle acceptance.

Clarifications
- Stable fields include: timeout_ms (0 on unavailable), reason on CooperativeTimedOut when applicable, and total_ms on ShutdownComplete.
- Maintain per-VM isolation and responsiveness; events must remain ordered per VM.

Next slice
- Implement available-channel cooperative sequencing: emit CooperativeAttempted(method, timeout_ms) → CooperativeSucceeded | CooperativeTimedOut before TERM/KILL, honoring configurable waits. Preserve isolation and live streaming.

Acceptance refinements
- Exit success even when some VMs escalate to forced, provided all targets are stopped.
---

---

