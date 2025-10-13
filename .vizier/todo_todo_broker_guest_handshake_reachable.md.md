Progress link — Snapshot v0.7.7

- Broker logs now include durable bus `publish` persistence outcomes, and CLI gained `bus tail` to surface them, which supports observability efforts. Handshake log/event lines with capabilities and session outcomes remain to be finalized for deterministic automation consumption.

Acceptance reminder
- Add deterministic handshake log/events that mention observed capabilities and whether a bus session was established; finalize help/legend and JSON field docs. Cross-link status loader invariants (non-blocking, `reachable` + `last_handshake_age_ms`).

Anchors
- src/core/broker.rs (handshake/session logs); src/core/status.rs; src/app/status.rs; src/core/logs.rs; BUS.md.

---

