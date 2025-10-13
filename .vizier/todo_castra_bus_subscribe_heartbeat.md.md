
---
Status update (post-commit 14b41ba)
- Delivered: subscribe handling with bounded limits, framed publish acks after durable append, heartbeat tracking with 60s timeout, session timeout/cleanup on disconnect, and non-blocking status fields (bus_subscribed, last_publish_age_ms, last_heartbeat_age_ms). Legend/help and BUS.md updated. Tests cover freshness reporting.

Remaining follow-ups (narrowed scope)
- Back-pressure observability: add explicit log/Event when per-session queue back-pressure triggers drops/retries/disconnect so operators can diagnose.
- CLI/docs polish: ensure `castra bus tail` help references BUS status signals and heartbeat behavior; add an example showing BUS and BUS AGE columns.
- Edge-case tests: future-dated timestamps already diagnosed; add tests for heartbeat timeout recovery (re-handshake) and subscribe state clearing on reconnect.

Acceptance delta
- Core acceptance satisfied. Issue remains open only for observability and edge-case test coverage.

Thread link: Thread 13 — Castra Bus — snapshot v0.7.9.

---

