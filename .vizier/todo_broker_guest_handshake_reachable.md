# Thread 3 — Host communication channel
Snapshot: v0.7 (Current)

Goal
- Introduce a minimal broker ↔ guest handshake so `castra status` can surface BROKER state = reachable (beyond waiting/offline).

Why (tension)
- Snapshot Thread 3 and the status legend promise a reachable state, but no guest handshake exists. Users cannot tell when the guest has connected.

Desired behavior (product level)
- Each VM runs a tiny guest-side agent or uses a one-shot init script to connect to the host broker on boot and present an identity (vm name or token).
- The broker records current reachable VMs and exposes this to status.
- `castra status` shows BROKER = reachable when the guest handshake has been observed within a reasonable freshness window; otherwise waiting while broker is up.
- Logs include a line per connection with a timestamp and VM identity.

Acceptance criteria
- With VM(s) running and broker up, status shows reachable for VMs whose guest connected; stopping the guest removes or ages out reachability back to waiting.
- On fresh boot, logs include a deterministic greeting line (e.g., "hello from vm:<name>") that appears under [host-broker].
- If broker is offline, status continues to show offline; no false-positive reachable states.

Scope and anchors (non-prescriptive)
- Anchors: src/app/status.rs (legend/BROKER col), src/core/status.rs (row assembly), src/app/logs.rs (log prefixes), broker core implementation.
- Implementation is open: handshake framing, identity, and freshness tracking can be minimal; prioritize stability and privacy (localhost only).
Expand to full product-level spec.

Thread 3 — Host communication channel
Snapshot: v0.7.1 (Current)

Goal
- Replace pid-file-only BROKER wait with a real broker↔guest handshake and a freshness-window-backed `reachable` state; expose last-handshake age in StatusOutcome for UIs.

Why (tension)
- Current status waits when broker.pid exists, but there is no liveness or freshness check; UIs can’t tell if the guest agent is actually reachable.

Desired behavior (product level)
- Runtime tracks last successful broker↔guest handshake; `reachable` is true only if the last handshake age is within a freshness window (duration TBD, configurable or sane default).
- `castra status` includes `reachable` and `last_handshake_age` (or timestamp) in OperationOutput<StatusOutcome>.
- Logs/events include handshake start/success/failure with reasons; stale state is communicated clearly.

Acceptance criteria
- When the broker is up and the guest agent responds, status shows reachable=true and a small age; stopping the agent causes age to grow past the window and reachable=false without hanging commands.
- On cold start, status reflects transitioning state without indefinite waits; a bounded wait may be used for initial handshake but must not block unrelated operations.
- Age/timestamp fields are stable and script-friendly; help text documents their meaning.

Scope and anchors (non-prescriptive)
- Anchors: src/core/broker.rs (handshake/keepalive), src/core/status.rs (StatusOutcome), src/core/events.rs (Events), src/app/status.rs (rendering).
- Keep transport/mechanism open; focus on observable liveness semantics and output.


---

Field naming + status behavior
- StatusOutcome fields: include `reachable: bool` and either `last_handshake_age_ms: u64` or `last_handshake_ts: SystemTime`. Names should be script-friendly.
- Bounded initial wait: status may wait briefly on first observation but must not block unrelated operations; subsequent calls are non-blocking and reflect staleness via age growth.

Anchors addition
- src/core/status.rs (StatusOutcome extension) and src/app/status.rs (rendering new fields).

---

Thread 3 — Host communication channel. Snapshot v0.7.2 reference.

Tension
- `status` reflects only broker PID existence; there is no handshake freshness, causing misleading "healthy" even when the guest/broker link is stale.

Evidence
- src/core/status.rs:45; src/core/broker.rs:64 — status waits on pid-only, no age/freshness tracked.

Change (product-level)
- Add non-blocking freshness to status. A bounded initial probe (<500ms) is allowed on first call; subsequent calls must be non-blocking and report staleness via age.
- Surface fields: reachable: bool; last_handshake_age_ms: u64 in the status output/JSON.
- CLI help/legend explains semantics of reachable and age.

Acceptance criteria
- Running `castra status` shortly after broker restart shows reachable=true and age≈0..500ms; repeated calls show age increasing without blocking.
- If broker is down or handshake not observed within probe window, reachable=false and age reports time since last successful handshake (or null/"-" if none), with clear legend.
- JSON output includes fields with stable names. Text UI shows age in human units and a legend line.

Anchors
- src/core/status.rs; src/core/broker.rs; src/app/status.rs (help/legend).Cross-link: BOOTSTRAP.md relies on `reachable=true` and `last_handshake_age_ms` to trigger host-side bootstrap. Acceptance should ensure `status --json` fields are stable enough for external automation to poll at ~2s cadence without blocking. Future: sessions may upgrade to bus-v1 (Thread 13) but handshake-only must remain supported.

---

Thread 3 — Broker reachability and deterministic handshake signals (Snapshot v0.7.7)

Context
- Status JSON already exposes `reachable` and `last_handshake_age_ms`. Handshake parses capability strings and conditionally starts bus sessions. Deterministic broker logging is in place.
- Host CLI for bus landed; observability path exists via `bus tail`, but handshake-specific logs/events are not yet standardized.

Tension
- Operators and automation need reliable, non-blocking freshness signals and machine-parseable handshake evidence, including observed capabilities and session establishment results.

Product change (behavioral)
- Preserve non-blocking `status` call semantics while ensuring fields are updated by periodic broker-side handshakes.
- Emit deterministic handshake Events/log lines including: vm name, capabilities (sorted unique), session outcome (ok/declined), and reason on decline.
- Update help/legend and JSON field docs to define staleness thresholds and polling cadence expectations (~2s baseline) without coupling to runtime.

Acceptance criteria
- With and without `capabilities` present, `status --json` remains non-blocking and `reachable`/`last_handshake_age_ms` semantics unchanged.
- On handshake, broker writes an ordered, deterministic line to its log and emits an Event containing vm, capabilities, and session outcome; lines are covered by tests.
- Docs: status legend updated; BOOTSTRAP.md references handshake evidence for bootstrap triggers.

Anchors
- src/core/status.rs; src/core/broker.rs (handshake codepaths/logging); src/app/status.rs; src/core/logs.rs; docs/BOOTSTRAP.md.

Thread links
- Depends on Snapshot v0.7.7 current state. Feeds Thread 12 (bootstrap triggers) and Thread 13 (bus session lifecycle reporting).Standardize handshake evidence and finalize status documentation.

Describe deterministic, machine-parseable handshake signals and keep status non-blocking. On each guest↔broker handshake, write a single, ordered log line and emit a structured Event including vm identity, observed capabilities, and session outcome. Update CLI legend/docs to define `reachable`/`last_handshake_age_ms` semantics and polling expectations. (thread: broker-reachability — snapshot v0.7.8/Thread 3)

Acceptance Criteria:
- Status behavior:
  - `castra status --json` remains non-blocking; `reachable` and `last_handshake_age_ms` semantics unchanged.
  - Help/legend documents meanings of both fields, the staleness threshold, and recommended polling cadence (~2s).
- Handshake log line:
  - On handshake, broker writes one deterministic line including: timestamp, vm identity, capabilities (sorted, deduped), session outcome (ok/declined), and decline reason if any.
  - Line appears under the host broker log prefix and is stable for parsing; covered by a test.
- Handshake Event:
  - A structured Event is emitted on handshake with the same fields (vm, capabilities, session outcome, optional reason).
  - Event is available via existing logs/events surfaces and is machine-parseable; covered by a test.
- Docs:
  - Status JSON field docs updated to include `reachable` and `last_handshake_age_ms` semantics and examples.
  - BOOTSTRAP.md references handshake evidence for bootstrap triggers.

Pointers:
- src/core/broker.rs (handshake/logging)
- src/core/status.rs; src/app/status.rs (legend/help)
- src/core/events.rs; src/core/logs.rs
- docs/BOOTSTRAP.md; README/help text