
---
Progress
- ShutdownComplete now includes total_ms and `castra down` renders durations. Cooperative attempt sequence remains to be implemented.
---


---



---
Progress update (v0.8.5)
- ShutdownComplete now includes total_ms and `castra down` renders per-VM shutdown durations. This lays groundwork for ordered, timed shutdown reporting.
- Cooperative attempt sequence remains to be implemented; acceptance criteria unchanged.

Next acceptance slice
- Emit CooperativeAttempted and CooperativeSucceeded/CooperativeTimedOut prior to TERM/KILL, respecting configurable timeouts and preserving per-VM isolation.
---

---
Recent progress (tests)
- Added unix-gated cooperative shutdown tests that simulate QMP-driven success and timeout/escalation paths, asserting ordered events and outcomes to lock in the lifecycle contract.
---


---
