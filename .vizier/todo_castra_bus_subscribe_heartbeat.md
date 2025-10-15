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
- Only acknowledge host CLI publish after durable append to the bus logs; enforce bounded per-session queues to prevent starvation.Expose back-pressure signals and add reconnection/timeout edge-case tests for Castra Bus.
Make back-pressure explicitly observable via logs/events without changing the non-blocking status contract; add tests covering heartbeat timeouts, cleanup, and reconnection so subscription state is deterministic and recoverable. Keep host CLI behavior and durable publish acks unchanged. (thread: Castra Bus — snapshot v0.7.9/Thread 13)

Acceptance Criteria
- Back-pressure observability:
  - When a session hits bounded queues or slow-consumer thresholds, broker emits deterministic log lines and structured Events with reason (e.g., queue_full, slow_consumer) and action (retry|disconnect).
  - If a disconnect is triggered by back-pressure, subscription state is cleared and the reason is visible in logs/events.
- Status non-blocking and stable:
  - `castra status --json` remains non-blocking; `bus_subscribed`, `last_publish_age_ms`, and `last_heartbeat_age_ms` semantics unchanged and documented.
  - BUS and BUS AGE columns continue to render without blocking, including during back-pressure and timeouts.
- Edge-case tests:
  - Heartbeat timeout triggers session cleanup; subsequent status shows unsubscribed and ages reset/diagnosed; no leaked state.
  - Reconnection after timeout re-establishes subscription; ages reset appropriately; previous session state is not reused.
  - Under sustained slow consumer, publishes remain durably appended; per-session queue limits enforced without starving other sessions.
  - Concurrent sessions under load do not block status or host CLI operations; publish acks remain timely post-durability.
- Host UX and compatibility:
  - `castra bus publish` behavior unchanged (success only after durable append; actionable failures).
  - Guests without bus capability continue handshake-only with no regressions to `reachable`/`last_handshake_age_ms`.
- Documentation:
  - BUS.md updated with copy-pastable examples of back-pressure logs/events, timeout cleanup, and reconnection; legend/help notes how to observe these conditions.

Pointers
- src/core/broker.rs (session/back-pressure paths, cleanup)
- src/core/status.rs; src/app/status.rs (status fields and legend)
- src/core/events.rs; src/core/logs.rs (back-pressure and timeout signals)
- src/app/bus.rs; src/cli.rs (CLI behavior)