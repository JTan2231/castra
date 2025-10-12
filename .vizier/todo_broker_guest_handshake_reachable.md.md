Snapshot ref bump: v0.7.2. Clarify non-blocking status behavior and field naming.

- Status must remain responsive: no indefinite waits; initial observation may be bounded (<500ms), subsequent calls are non-blocking and reflect staleness via age growth.
- Field names stabilized: `reachable: bool`, `last_handshake_age_ms: u64`.
- Help/legend in src/app/status.rs should briefly explain "reachable" and "age" semantics.


---

