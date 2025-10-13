Thread 13 — Castra Bus (subscribe/ack/heartbeat/back-pressure) — Snapshot v0.7.8

Context
- Broker supports capability-gated bus sessions. Host CLI (`castra bus publish`, `castra bus tail`) is shipped with durable logs for publishes. Guests can publish; subscribe/heartbeat/back-pressure still pending.

Product outcome
- Broker supports subscribe/ack heartbeats with back-pressure and session timeout/cleanup while remaining compatible with handshake-only guests.
- Status gains non-blocking freshness fields: `bus_subscribed: bool`, `last_publish_age_ms`, `last_heartbeat_age_ms`.
- Host CLI remains fast; `bus publish` succeeds only after durable append.

Acceptance Criteria
- Broker session behavior:
  - Guests establish long-lived sessions; support `subscribe`, `ack`; idle sessions send periodic heartbeats.
  - Back-pressure triggers bounded retries or clean disconnect; timeouts/cleanup remove server state.
  - Disconnects clear subscription state deterministically; heartbeats/timeouts observable in logs/events.
- Status fields:
  - `castra status --json` includes `bus_subscribed`, `last_publish_age_ms: u64`, `last_heartbeat_age_ms: u64`; non-blocking and documented in help/legend.
- Host UX:
  - `castra bus publish` returns success only when the frame is durably appended; failures return non-zero with actionable diagnostics.
  - `castra bus tail` remains non-blocking and reflects subscription state via log copy.
- Compatibility:
  - Guests without bus capabilities continue in handshake-only mode with no regressions to `reachable`/`last_handshake_age_ms`.

Pointers
- src/core/broker.rs (session loop)
- src/core/status.rs; src/app/status.rs (fields + legend/help)
- src/core/events.rs; src/core/logs.rs (observable signals)
- src/app/bus.rs; src/cli.rs (host UX)

Safety note
- Only acknowledge host CLI publish after durable append to the bus logs; enforce bounded per-session queues to prevent starvation.