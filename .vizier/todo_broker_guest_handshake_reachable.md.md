Progress note — Snapshot v0.7.6

- Partial delivery: StatusOutcome exposes `reachable` and `last_handshake_age_ms`; legend partially updated. Non-blocking guarantees and handshake log/event lines still pending tests.
- Evidence: snapshot and code surfaces (src/core/status.rs, src/core/broker.rs) reflect fields; help/legend updates noted.

Next steps
- Add deterministic handshake start/success/failure Events and corresponding host-broker log lines.
- Ensure bounded initial probe (<500ms) on first observation; subsequent calls non-blocking with age-based staleness.
- Finalize CLI help/legend and document JSON field semantics for automation (BOOTSTRAP.md relies on ~2s polling cadence).


---

