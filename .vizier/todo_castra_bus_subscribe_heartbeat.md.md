

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

