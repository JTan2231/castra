
---
Progress (v0.8.5+)
- Bootstrap runs concurrently per VM with live event streaming via a central channel; per-VM ordering and isolation are preserved.
- Handshake timeout failures are observable and durable: Failed WaitHandshake step → BootstrapFailed with a single durable run log; polling uses sub-second slices respecting configured deadlines.

Next slice
- Add per-invocation bootstrap overrides (e.g., `castra up --bootstrap <mode>`), and document emitted event/log payloads for automation.

Acceptance clarifications
- Outcomes are returned in the original VM order; first error across workers is captured while allowing others to proceed.
---


---

