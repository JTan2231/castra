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

