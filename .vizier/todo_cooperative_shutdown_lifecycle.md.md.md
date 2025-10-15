
Update — unavailable cooperative channel semantics shipped
- Runtime now emits CooperativeAttempted with timeout_ms=0 when cooperative method/channel is unavailable, followed by CooperativeTimedOut(reason: ChannelUnavailable) without waiting, before escalating.
- Tests cover QMP success, QMP timeout→TERM, and unavailable-channel paths with ordered events and field assertions.

Next slice refinement
- Implement available-channel cooperative attempt in runtime with bounded wait honoring CLI/opts; emit CooperativeSucceeded or CooperativeTimedOut(reason: Timeout) before TERM/KILL.
- Ensure per-VM isolation and live streaming are preserved under mixed outcomes (some graceful, some forced).
- Acceptance adds: CooperativeTimedOut includes reason { Timeout | ChannelUnavailable } and records waited_ms; ShutdownComplete includes outcome and total_ms (already present).


---

