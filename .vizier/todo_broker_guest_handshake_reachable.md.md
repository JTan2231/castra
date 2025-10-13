Status update — delivery and remaining work (Snapshot v0.7.5)
- Delivery: `status --json` now includes `reachable: bool` and `last_handshake_age_ms`; legend/copy partially updated.
- Remaining: codify non-blocking guarantees with tests (bounded initial probe, subsequent calls non-blocking), and emit deterministic handshake logs/events (start/success/stale) with timestamps. Finalize help/legend language for automation users.

Acceptance refinement
- Tests assert: no command blocks longer than the configured probe window when broker/guest are absent; age grows monotonically between calls; logs include a single-line handshake success with VM identity.

Cross-links
- Thread 12 (bootstrap): consumers rely on stable field names; verify semantics at ~2s polling cadence.

---

