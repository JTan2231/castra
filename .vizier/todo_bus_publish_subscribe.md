# Thread 13 — Castra Bus (host-centric pub/sub on the broker)

Context
- Source: BUS.md details a host-mediated message bus layered on the existing broker reachability handshake.
- Depends on: Thread 3 (broker freshness/handshake); should remain compatible with handshake-only guests via capability flags.
- Anchors: `src/core/broker.rs`, `src/core/status.rs`, `src/core/events.rs`, `src/core/logs.rs`, `src/app/logs.rs`, `src/app/status.rs`, CLI additions under `src/cli.rs`.

Product outcome
- Guests can publish structured events to the host and optionally subscribe to broadcasts via a long-lived session to the broker.
- Operators can `castra bus publish` and `castra bus tail` from the host, with fast, non-blocking UX and durable logs.

Acceptance criteria
- Protocol: guests send `hello vm:<name> capabilities=bus-v1`; broker replies `ok session=<token>`. Framed, length-prefixed JSON messages support `publish`, `subscribe`, `ack`, and `heartbeat`.
- Status: `castra status --json` extends with per-VM bus fields (e.g., `bus_subscribed: bool`, `last_publish_age_ms`), while preserving `reachable` and `last_handshake_age_ms` (Thread 3 contract).
- Observability: bus messages are persisted under `<state_root>/logs/bus/*.log` with vm/timestamp metadata; `castra bus tail` follows these streams.
- Performance: end-to-end delivery target under 200 ms under nominal load; idle sessions heartbeated to keep freshness.
- Safety: session timeouts and back-pressure limits are enforced; disconnects clear subscription state.
- Compatibility: guests without `capabilities=bus-*` continue to use handshake-only mode; no regressions to reachability.

Notes
- Keep transport/serialization choices open; JSON frames are the default sketch, but implementation can evolve if constraints demand. Expose capability versioning to allow future evolution without breaking older guests.
- Cross-links: bootstrap daemon (Thread 12) may ride the bus for triggers in the future but should not block initial delivery.Thread 13 — Castra Bus (host CLI landed; broker subscribe/heartbeat pending) — Snapshot v0.7.7

Context
- Broker supports capability-gated bus sessions and persists guest `publish` frames to per-VM/shared logs. New host CLI: `castra bus publish` and `castra bus tail`, persisting host-originated frames and tailing logs.

Tension
- Interactive and automated consumers need subscribe/heartbeat/back-pressure guarantees and bus freshness signals in status, without regressing reachability guarantees.

Product change (behavioral)
- Broker session loop to implement `subscribe`, `ack`, heartbeat cadence, back-pressure limits, and session timeout/cleanup. Non-blocking by design; compatibility preserved for handshake-only guests.
- Status JSON extended with per-VM bus fields: `bus_subscribed: bool`, `last_publish_age_ms`, `last_heartbeat_age_ms`.
- CLI remains fast: `bus tail` reflects subscription state via log copy; `bus publish` returns success when logs are durably persisted.

Acceptance criteria
- Heartbeats and timeouts observable in logs; back-pressure causes bounded retries or disconnect with clear logging.
- Status reflects bus freshness without blocking; fields documented in help/legend.
- Disconnected sessions clear subscription state deterministically.

Anchors
- src/core/broker.rs (session loop); src/core/status.rs (new fields); src/core/events.rs; src/core/logs.rs; src/cli.rs (no changes required for current scope).

Thread links
- Builds on Thread 3 signals; future cross-link with Thread 12 bootstrap triggers possible.Deliver subscribe/heartbeat/back‑pressure and expose bus freshness in status.
Enable broker sessions to support subscribe/ack heartbeats with back‑pressure and timeouts, while preserving compatibility for handshake‑only guests. Extend status with per‑VM bus freshness fields and keep host CLI fast and non‑blocking; `bus publish` succeeds only on durable append. (thread: castra-bus — snapshot v0.7.8/Thread 13)

Acceptance Criteria:
- Broker session behavior:
  - Guests can establish a long‑lived session and issue subscribe/ack; idle sessions send periodic heartbeats.
  - Back‑pressure triggers bounded retries or clean disconnect; session timeout/cleanup removes server‑side state.
  - Disconnects deterministically clear subscription state; events/logs show heartbeats, back‑pressure, and timeouts.
- Status fields:
  - `castra status --json` includes per‑VM fields: `bus_subscribed: bool`, `last_publish_age_ms: u64`, `last_heartbeat_age_ms: u64`.
  - Calls are non‑blocking; fields are documented in help/legend and remain stable for scripts.
- Host UX:
  - `castra bus publish` returns success only when the frame is durably appended to per‑VM/shared logs; failures return non‑zero with actionable diagnostics.
  - `castra bus tail` remains fast and non‑blocking; reflects subscription state via log copy.
- Compatibility:
  - Guests without bus capabilities continue to operate in handshake‑only mode with no regressions to `reachable`/`last_handshake_age_ms`.

Pointers:
- src/core/broker.rs (session loop: subscribe/ack/heartbeat/back‑pressure/timeout)
- src/core/status.rs; src/app/status.rs (new fields + legend/help)
- src/core/events.rs; src/core/logs.rs (observable signals)
- src/app/bus.rs; src/cli.rs (host UX)

Implementation Notes (safety/correctness):
- Only acknowledge host CLI publish after durable append (fsync or equivalent policy) to the bus logs.
- Enforce bounded queues per session; apply fair back‑pressure to prevent broker starvation.
- Status freshness must derive from broker‑maintained timestamps; do not block status on live sessions.