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

Add denial/timeout handshake evidence and non-blocking status guarantee.
Describe and enforce behavior when a handshake is denied or times out: emit deterministic broker log lines and structured Events (including reason when applicable); keep status fields non-blocking with unchanged semantics; add docs examples for success vs denial/timeout. (thread: broker-reachability)

Acceptance Criteria
- Status non-blocking:
  - `castra status --json` never blocks regardless of broker state (reachable, denial, timeout, unreachable) and returns within a bounded time.
  - `reachable` and `last_handshake_age_ms` semantics remain unchanged and documented; no new fields required to observe failures/timeouts.
- Denied handshake evidence:
  - Broker emits exactly one deterministic log line per denied handshake with: timestamp, VM identity, normalized capabilities (sorted/deduped), session_outcome=denied, reason.
  - A structured Handshake Event is emitted with fields: vm, capabilities (normalized), session_outcome=denied, reason (non-empty).
  - Tests cover log line stability and Event fields.
- Timeout handshake evidence:
  - Broker emits exactly one deterministic log line per timeout with: timestamp, VM identity, normalized capabilities, session_outcome=timeout (no reason required).
  - A structured Handshake Event is emitted with fields: vm, capabilities (normalized), session_outcome=timeout.
  - Tests cover log line stability and Event fields.
- Documentation:
  - Add copy-pastable examples of success, denial (with reason), and timeout handshake logs and Events.
  - Update status help/legend confirming freshness semantics and non-blocking guarantee; keep field names stable.

Pointers
- src/core/broker.rs (handshake denial/timeout paths)
- src/core/events.rs (Handshake Event variants/fields)
- src/core/logs.rs (deterministic broker log lines)
- src/app/status.rs (legend/help copy)
- docs/BUS.md and docs/status JSON fields/examples