

Update (Snapshot v0.7.9): Core subscribe/ack/heartbeat shipped.

- Delivered: Session subscribe handling, durable publish acks, heartbeat tracking with 60s timeout, session timeout/cleanup, and non-blocking BUS status signals (`bus_subscribed`, `last_publish_age_ms`, `last_heartbeat_age_ms`). Status UI shows BUS and BUS AGE columns; BUS.md updated.
- Remaining scope:
  - Add explicit back-pressure observability via logs/events with bounded queue signals and host-visible diagnostics.
  - Expand edge-case tests: heartbeat timeout recovery, reconnect clears subscription state, publish during disconnect, and bounded retries behavior.
  - Ensure status remains non-blocking across reconnection/timeout scenarios; add tests.

Acceptance delta
- Extend acceptance to include back-pressure observability and reconnection/timeout edge-case tests while keeping existing success-path tests passing.

Pointers unchanged
- src/core/broker.rs (session loop)
- src/core/status.rs; src/app/status.rs (fields + legend/help)
- src/core/events.rs; src/core/logs.rs (observable signals)
- src/app/bus.rs; src/cli.rs (host UX)

Thread link: Thread 13 — Castra Bus.


---

Expose bus back-pressure signals and harden reconnection/timeout behavior without blocking status. (thread: Castra Bus — snapshot v0.7.9/Thread 13)

Describe and enforce observable behavior when sessions hit bounded queues or lose heartbeats: emit deterministic logs/events with reason and action; ensure publish durability is unaffected; validate cleanup and reconnection; keep status non-blocking with stable fields.

Acceptance Criteria
- Back-pressure observability:
  - When a session reaches bounded queue/slow-consumer thresholds, broker emits deterministic log lines and structured Events with reason (e.g., queue_full, slow_consumer) and action (retry|disconnect).
  - If disconnect occurs due to back-pressure, subscription state is cleared and the reason is visible via logs/events.
- Status contract (non-blocking, stable fields):
  - `castra status --json` remains non-blocking during normal, back-pressure, disconnect, and reconnection scenarios.
  - Fields `bus_subscribed`, `last_publish_age_ms`, and `last_heartbeat_age_ms` retain semantics and are documented in legend/help; BUS and BUS AGE columns render without blocking.
- Edge-case tests:
  - Heartbeat timeout triggers session cleanup; subsequent status shows unsubscribed and no leaked subscription state.
  - Reconnection after timeout establishes a fresh session; previous subscription state is not reused; ages reset appropriately.
  - Publish during disconnect/back-pressure remains durably appended; host CLI returns success only after durability; actionable diagnostics are emitted if any transient back-pressure affects delivery to subscribers.
  - Bounded retries behave as documented: retries occur up to a limit, then a clean disconnect is performed; other sessions are not starved.
  - Concurrent sessions under load do not block status or host CLI operations; publish acks remain timely post-durability.
- Documentation:
  - BUS.md includes copy-pastable examples of back-pressure and timeout/reconnect logs/events and clarifies how to observe these conditions.
  - Status legend/help explicitly references bus freshness fields and non-blocking guarantees.

Pointers
- src/core/broker.rs (session/back-pressure paths, timeout cleanup, reconnect handling)
- src/core/status.rs; src/app/status.rs (fields and legend/help)
- src/core/events.rs; src/core/logs.rs (structured Events and deterministic lines)
- src/app/bus.rs; src/cli.rs (host UX and diagnostics)