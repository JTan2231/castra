

Update (Snapshot v0.7.9)
- Status: Success path shipped. On successful guest↔broker handshake we now emit a deterministic broker-prefixed log line and a structured Handshake Event. `reachable` and `last_handshake_age_ms` remain non-blocking and stable; legend/help updated accordingly.
- Remaining scope: denial/timeout paths and examples.

Revised acceptance (remaining to close the thread)
- Denial/timeout evidence:
  - Emit the same deterministic log and Event on denied/failed/timeout handshakes, including `session_outcome=denied|timeout` and a stable `reason` where applicable.
  - Tests cover at least: capabilities missing/unsupported (denied), broker intentionally denying (policy), and handshake timeout.
- Docs/examples:
  - Add examples for both success and denial/timeout cases in BUS.md/README, showing the log line and Event fields.
- Non-blocking guarantee:
  - Confirm via test that `castra status --json` does not block even when the last handshake attempt is pending or failed; fields remain present with well-defined values.

Pointers unchanged
- src/core/broker.rs (handshake path)
- src/core/events.rs (Handshake Event)
- src/core/logs.rs (deterministic line)
- src/app/status.rs (legend/help)
- docs (status JSON fields, examples)

Thread link: broker-reachability — builds on Snapshot v0.7.9 success-path behavior.

---

