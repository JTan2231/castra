
---
Progress (v0.8.5+)
- `down` executes shutdowns concurrently per VM and streams events live to the UI.
- Runtime emits CooperativeAttempted with timeout_ms=0 when the cooperative method/channel is unavailable, followed by CooperativeTimedOut(reason: ChannelUnavailable) without waiting.
- ShutdownComplete now includes total_ms and is rendered in `down`.
- Unix-gated tests cover QMP success/timeout sequences and the unavailable-channel path, asserting ordered events and stable fields.

Next slice
- Implement available-channel cooperative sequencing in the runtime prior to TERM/KILL, honoring configurable timeouts and preserving per-VM isolation and live streaming.

Acceptance clarifications
- On unavailable cooperative channels, emit CooperativeAttempted(timeout_ms=0) → CooperativeTimedOut(reason: ChannelUnavailable) then proceed to escalation without delay.
- Stable fields: timeout_ms, reason (when timed out), and total_ms on ShutdownComplete.
- Forced shutdowns count as success for `castra down` exit status; a warning is printed listing affected VMs.
---


---

