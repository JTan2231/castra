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
- Cross-links: bootstrap daemon (Thread 12) may ride the bus for triggers in the future but should not block initial delivery.