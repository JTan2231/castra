
Update — unavailable cooperative channel semantics shipped
- Runtime now emits CooperativeAttempted with timeout_ms=0 when cooperative method/channel is unavailable, followed by CooperativeTimedOut(reason: ChannelUnavailable) without waiting, before escalating.
- Tests cover QMP success, QMP timeout→TERM, and unavailable-channel paths with ordered events and field assertions.

Update — forced shutdown success semantics
- `castra down` now exits successfully when VMs require forced termination, surfacing the forced set via stderr while preserving ordered lifecycle events.

Next slice refinement
- Implement available-channel cooperative attempt in runtime with bounded wait honoring CLI/opts; emit CooperativeSucceeded or CooperativeTimedOut(reason: Timeout) before TERM/KILL.
- Ensure per-VM isolation and live streaming are preserved under mixed outcomes (some graceful, some forced).
- Acceptance adds: CooperativeTimedOut includes reason { Timeout | ChannelUnavailable } and records waited_ms; ShutdownComplete includes outcome and total_ms (already present).


---

---
Progress update (v0.8.5+)
- Runtime now emits CooperativeAttempted with timeout_ms=0 when the cooperative channel is unavailable and CooperativeTimedOut with reason=ChannelUnavailable in that path. Added unix-gated tests asserting ordered events including escalation and ShutdownComplete.

Next slice (unchanged)
- Implement CooperativeAttempted → CooperativeSucceeded/CooperativeTimedOut sequencing for available channels before TERM/KILL with configurable waits.

Acceptance clarifications
- Ensure timeout_ms and reason fields are present and stable in CooperativeTimedOut; preserve per-VM isolation and live streaming during waits.
---


---

