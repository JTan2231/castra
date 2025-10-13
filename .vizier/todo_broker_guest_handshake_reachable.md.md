Progress — Snapshot v0.7.7

- Broker handshake now parses capability strings and records them; when `bus-v1` is present it returns a session token and spawns a framed session (commit 32d5039). Handshake log writing is deterministic/thread-safe; bus logs are created alongside broker logs. Status loader tolerates extra fields.

Tension (unchanged)
- Users need stable, non-blocking freshness signals while we add capabilities and sessions; logs/events should be deterministic for automation.

Desired behavior (product level)
- Status is non-blocking; exposes `reachable` and `last_handshake_age_ms` with well-defined staleness bounds.
- Deterministic handshake Events/log lines include observed capabilities and whether a bus session was established.
- Help/legend finalized; JSON field semantics documented for automation cadence (~2s polling baseline).

Acceptance criteria
- With/without `capabilities` present, status calls remain non-blocking and fields behave identically.
- Handshake produces ordered, deterministic log/event lines that include caps and session outcome.
- Docs updated in help/legend and BOOTSTRAP.md to reflect field semantics and cadence.

Anchors
- src/core/status.rs; src/core/broker.rs; src/app/status.rs; src/core/logs.rs.
