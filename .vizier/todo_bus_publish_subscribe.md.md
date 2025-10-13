
Progress — Snapshot v0.7.7
- Partial delivery landed (commit 32d5039): capability-aware handshake parses `capabilities` from guests; when `bus-v1` is present, broker issues a session token and spawns a framed session handler. Implemented length-prefixed JSON frames and `publish` persists to per-VM/shared bus logs. Heartbeat/subscription remain stubbed. Logging is now thread-safe and bus log directory is created.

Implications
- Preserves handshake-only behavior for guests without `bus-*` caps (compatibility acceptance still holds).
- Lays groundwork for exposing bus freshness in status; status loader currently tolerates/ignores caps.

Next steps
- Host CLI: `castra bus publish` and `castra bus tail` with durable follow semantics and filters by VM.
- Protocol rounding: add `subscribe`, `heartbeat`, and back-pressure handling; define session timeout + disconnect cleanup.
- Status fields: add `bus_subscribed` and `last_publish_age_ms` without regressing `reachable`/`last_handshake_age_ms`.
- Tests: end-to-end publish persistence and tailing latency target (<200ms nominal); capability flag gating; disconnect clears subscription state.


---

