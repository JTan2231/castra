Thread 3 — Broker reachability freshness and handshake evidence (Snapshot v0.7.8)

Problem/Tension
- We expose `reachable` and `last_handshake_age_ms` non-blocking in `castra status --json`, but lack deterministic handshake evidence for tools (logs/events). Docs/legend need to lock semantics.

Product change (behavioral)
- On each guest↔broker handshake, emit:
  - One deterministic, machine-parseable log line under the broker prefix.
  - One structured Event with stable fields.
- Update status help/legend and docs to define `reachable` and `last_handshake_age_ms` semantics, keeping them non-blocking and field names stable.

Acceptance Criteria
- Status behavior:
  - `castra status --json` remains non-blocking; `reachable` and `last_handshake_age_ms` semantics unchanged and documented.
  - Help/legend explains freshness window and capability presence without requiring capabilities.
- Handshake log line:
  - Contains: timestamp, VM identity, capabilities (sorted/deduped), session outcome (granted/denied), and optional denial reason.
  - Stable for parsers; covered by a test.
- Handshake Event:
  - Includes fields: vm, capabilities, session_outcome, optional reason; visible via logs/events surfaces; covered by a test.
- Docs:
  - README/docs updated with field names and examples for `reachable` and `last_handshake_age_ms`.

Pointers
- src/core/broker.rs (handshake path)
- src/core/events.rs (Handshake Event)
- src/core/logs.rs (deterministic line)
- src/app/status.rs (legend/help)
- docs (status JSON fields)

Thread link: broker-reachability — depends on Snapshot v0.7.8 current fields.Update (Snapshot v0.7.9): Success-path shipped.

- Delivered: Deterministic success handshake log line and structured Event; capabilities normalized; legend/docs updated for `reachable` and `last_handshake_age_ms` semantics. Non-blocking behavior preserved.
- Remaining scope tightened:
  - Add denial/timeout-path coverage: tests validating deterministic denial log line + Event fields (including reason) and that status remains non-blocking during handshake failures/timeouts.
  - Add example snippets in docs showing success vs denial/timeout logs/events (copy-pastable).
  - Add explicit non-blocking guarantee test ensuring `castra status --json` does not block regardless of broker state.

Acceptance delta
- Extend acceptance with denial/timeout tests + examples; keep existing success-path tests passing.

Pointers unchanged
- src/core/broker.rs (handshake path)
- src/core/events.rs (Handshake Event)
- src/core/logs.rs (deterministic line)
- src/app/status.rs (legend/help)
- docs (status JSON fields + examples)

Thread link: Thread 3 — Broker reachability.

---

