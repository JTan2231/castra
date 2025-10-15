
---
Refinement (edge cases + docs)
- Tests to exercise reconnection under back-pressure and heartbeat timeout edges; verify subscription state resets and last_*_age_ms fields behave monotonically.
- BUS.md examples showcasing back-pressure WARN lines and structured bus-events.jsonl records (reason/action/detail) with guidance.
- Ensure `status` remains non-blocking while under stress (slow consumer, queue full, heartbeat timeout).
- Tail/publish UX retains stable formatting during churn.
Cross-links: Thread 13 in snapshot (Castra Bus).

---

