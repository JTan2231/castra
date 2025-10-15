
---
Refinement (events + acceptance)
- Add a cooperative shutdown phase prior to TERM→KILL, attempting ACPI power button and/or guest agent request when available.
- Emit ordered lifecycle events: ShutdownRequested → GuestCooperativeAttempted → GuestCooperativeConfirmed | GuestCooperativeTimeout → HostTerminate → HostKill (only if needed) → ShutdownComplete.
- Timeouts: configurable pre-stop wait and cooperative deadline; honor existing lifecycle waits.
- Acceptance:
  - When guest cooperates, process exits cleanly without TERM/KILL and events reflect this path.
  - On timeout, proceed to TERM then KILL, with clear events and durations recorded.
  - Status remains responsive; no blocking on hung guests.
- Surfaces: src/core/runtime.rs; src/core/events.rs; src/app/down.rs; options.
Cross-links: Thread 2 in snapshot (Lifecycle gap).

---

