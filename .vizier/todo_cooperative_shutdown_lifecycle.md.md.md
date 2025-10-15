
---
Update (v0.8.5+): concurrency + tests

Progress
- `down` now shuts down VMs in parallel and streams events live to the reporter, preserving per‑VM ordering while improving responsiveness.
- Added unix‑only tests that simulate QMP to assert cooperative shutdown event ordering and fields for both success and timeout/escalation paths.

Next slice (unchanged in essence, now scoped to runtime)
- Implement real CooperativeAttempted/CooperativeSucceeded/CooperativeTimedOut emission in src/core/runtime.rs before TERM/KILL, using configurable timeouts from options.

Acceptance supplement
- Maintain per‑VM isolation under concurrency: long‑running guests do not block others; event sequences remain individually ordered.
- Ensure JSON output reflects new events with stable field names and integrates with existing ShutdownComplete.total_ms.

Anchors
- src/core/runtime.rs; src/core/events.rs; src/core/options.rs; src/core/reporter.rs; src/app/down.rs.
---


---

