Progress update — Snapshot v0.7.7

- Host CLI landed: `castra bus publish` and `castra bus tail` are implemented (src/cli.rs, src/app/bus.rs, src/core/operations/bus.rs). They write/follow durable logs under `<state_root>/logs/bus` and reuse log follower UX. Tests added for CLI parsing.
- Broker side remains partial: guests with `capabilities=bus-v1` can publish; subscribe/heartbeat/back-pressure/session timeouts are still stubbed. No status fields yet for bus freshness.

Acceptance criteria adjustments (what’s now satisfied)
- Operators can `castra bus publish` (host-originated frames persisted to shared and vm-targeted logs) and `castra bus tail` (shared/per-VM streams; follow mode). Performance and non-blocking UX preserved.
- Observability: persisted JSON lines include timestamp/vm/topic/payload.

Remaining to deliver
- Protocol: implement `subscribe`, `ack`, heartbeat cadence, and back-pressure handling in broker session loop; define session timeout/cleanup behavior.
- Status: extend `status --json` with per-VM bus freshness (e.g., `last_publish_age_ms`, `bus_subscribed`), keeping Thread 3 invariants.
- Safety: enforce limits and clear subscription state on disconnect; document capability versioning.

Anchors
- src/core/operations/bus.rs (host ops); src/app/bus.rs (CLI); src/cli.rs (args); src/core/broker.rs (session loop, stubs to evolve); BUS.md.


---

