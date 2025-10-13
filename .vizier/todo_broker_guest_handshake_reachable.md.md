Progress sync — Snapshot v0.7.8

- Fields `reachable` and `last_handshake_age_ms` are present in status JSON; bus capability negotiation is live for guests advertising `bus-v1`. Focus shifts to deterministic handshake Events/log lines (vm, capabilities, session outcome) and legend/docs updates. Acceptance remains non-blocking semantics with stable field names.

Update — v0.7.8 housekeeping

- Scope tightened: add concrete acceptance for standardized handshake logs/events and help legend.

Acceptance additions
- On guest handshake, broker emits a single-line log and a structured Event including: vm name, observed capabilities (array), and bus session outcome (granted/denied with reason). These appear under src/core/logs.rs and are visible via `castra logs --handshakes`.
- Status help/legend updated to define `reachable` and `last_handshake_age_ms` semantics (non-blocking freshness), and to mention capabilities presence without requiring them.
- JSON field docs updated in README or docs to match field names.

Anchors refinement
- src/core/broker.rs (handshake path); src/core/events.rs (new Handshake Event); src/core/logs.rs (deterministic line); src/app/status.rs (legend/help).