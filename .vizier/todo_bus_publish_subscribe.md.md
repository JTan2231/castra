# Thread 13 — Castra Bus (host-centric pub/sub on the broker)

Context
- Broker supports capability-gated bus sessions and persists guest `publish` frames to per-VM/shared logs. Host CLI is present: `castra bus publish` and `castra bus tail`.

Tension
- Consumers need subscribe/heartbeat/back-pressure guarantees and freshness signals in status, without regressing reachability. Host operations must confirm durable append semantics.

Product change (behavioral)
- Broker session loop to implement `subscribe`, `ack`, heartbeat cadence, back-pressure limits, and session timeout/cleanup. Compatibility preserved for handshake-only guests.
- Status JSON extended with per-VM bus fields: `bus_subscribed: bool`, `last_publish_age_ms`, `last_heartbeat_age_ms`.
- Host: `bus publish` succeeds only when frames are durably appended to logs. `bus tail` remains fast and non-blocking.

Acceptance criteria
- Heartbeats/timeouts observable in logs; back-pressure triggers bounded retries or disconnect with clear logging.
- Status reflects bus freshness without blocking; fields documented in help/legend.
- Disconnected sessions clear subscription state deterministically.
- `bus publish` returns non-zero exit on failed durable append with actionable diagnostics.

Anchors
- src/core/broker.rs (session loop); src/core/status.rs (fields); src/core/events.rs; src/core/logs.rs; src/app/bus.rs; src/cli.rs.
