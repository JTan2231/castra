
Update (Snapshot v0.7.9)
- Status: Core delivered. Broker now supports subscribe/ack with durable publish acks, periodic heartbeats with a 60s timeout, and deterministic session cleanup on disconnect/timeout. Status exposes `bus_subscribed`, `last_publish_age_ms`, and `last_heartbeat_age_ms`; BUS and BUS AGE columns shipped with legend/help.
- Remaining scope: explicit back-pressure observability (logs/events) and reconnection/timeout edge-case tests.

Revised acceptance (remaining)
- Observability:
  - Emit clear logs/events when per-session queues apply back-pressure, including bounded retries vs clean disconnect, with stable field names for scripts.
  - Add diagnostics for future-dated timestamps or clock skew affecting AGE fields.
- Tests:
  - Cover heartbeat timeout recovery, subscribe state clearing on reconnect, and back-pressure paths (retry vs disconnect).
- Docs:
  - BUS.md gains a section on back-pressure semantics and examples of the new logs/events.

Pointers unchanged
- src/core/broker.rs; src/core/status.rs; src/app/status.rs; src/core/events.rs; src/core/logs.rs; src/app/bus.rs; src/cli.rs; BUS.md.


---

