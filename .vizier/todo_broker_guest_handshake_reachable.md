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
