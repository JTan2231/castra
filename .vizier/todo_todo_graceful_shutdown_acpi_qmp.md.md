Snapshot v0.7.4 note — CLI copy mentions graceful powerdown attempt

Status
- Help text alludes to attempting a graceful powerdown during `down`, but cooperative shutdown is not confirmed in runtime yet.

Adjustment
- Keep acceptance focused on observable Events and actual behavior; ensure implementation matches the documented promise before release. Tests should guard that Events are emitted in order and that timeouts are honored.

Cross-link
- Align with CLEAN safety: running-process guard relies on accurate lifecycle state and Events.

---

