Progress sync — Snapshot v0.7.8

- Fields `reachable` and `last_handshake_age_ms` are present in status JSON; bus capability negotiation is live for guests advertising `bus-v1`. Focus shifts to deterministic handshake Events/log lines (vm, capabilities, session outcome) and legend/docs updates. Acceptance remains non-blocking semantics with stable field names.

Update — v0.7.8 housekeeping

- Scope tightened: add concrete acceptance for standardized handshake logs/events and help legend.

Acceptance additions
- On guest handshake, broker emits a single-line log and a structured Event including: vm name, observed capabilities (array), and bus session outcome (granted/denied with reason). These appear under src/core/logs.rs and are visible via `castra logs --handshakes`.
- Status help/legend updated to define `reachable` and `last_handshake_age_ms` semantics (non-blocking freshness), and to mention capabilities presence without requiring them.
- JSON field docs updated in README or docs to match field names.

Anchors refinement
- src/core/broker.rs (handshake path); src/core/events.rs (new Handshake Event); src/core/logs.rs (deterministic line); src/app/status.rs (legend/help).Standardize handshake evidence and finalize status legend/docs.

Ensure broker emits a deterministic, machine-parseable handshake log line and a structured Event on each guest↔broker handshake, including VM identity, observed capabilities, and bus session outcome, while keeping status non-blocking and existing field names stable. Update CLI legend/help and docs to define `reachable`/`last_handshake_age_ms` semantics. (thread: broker-reachability — snapshot v0.7.8/Thread 3)

Acceptance Criteria:
- Status behavior:
  - `castra status --json` remains non-blocking; `reachable` and `last_handshake_age_ms` semantics unchanged and documented.
  - Help/legend explains the freshness window and notes that capabilities may be present but are not required.
- Handshake log line:
  - On handshake, broker writes one deterministic line containing: timestamp, VM identity, capabilities (sorted, deduped array), session outcome (granted/denied), and denial reason if applicable.
  - Line appears under the host broker log prefix and is stable for parsers; covered by a test.
- Handshake Event:
  - A structured Event is emitted on handshake with the same fields (vm, capabilities, session outcome, optional reason).
  - Event is visible via existing logs/events surfaces and is machine-parseable; covered by a test.
- Docs:
  - JSON field docs updated to include `reachable` and `last_handshake_age_ms` names/semantics with examples.
  - Status help/legend updated to reflect non-blocking freshness semantics and polling expectations (~2s cadence).

Pointers:
- src/core/broker.rs (handshake path/logging)
- src/core/events.rs (Handshake Event)
- src/core/logs.rs (deterministic line)
- src/app/status.rs (legend/help)
- docs (README or dedicated status JSON fields page)