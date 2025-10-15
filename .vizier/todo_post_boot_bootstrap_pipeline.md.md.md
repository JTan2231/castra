
---
Progress update (v0.8.5+)
- Handshake timeout failures are observable and durable: WaitHandshake step marked Failed, BootstrapFailed emitted, and a single failed run log persisted with timeout detail.
- Unix-gated integration test exercises pipeline with stubbed ssh/scp/qemu covering step events, stamps, durable logs, and NoOp replay.

Next slice
- Emit BootstrapStarted/Completed(NoOp|Success) with durable per-step logs (connect, transfer, apply, verify) for a single-VM path behind an opt-in flag.
- Enforce idempotence stamp composed of (base_image_hash, bootstrap_artifact_hash) under state root; NoOp when unchanged.

Acceptance refinements
- Triggered exactly once per stamp change; safe re-runs emit NoOp without side effects.
- Events: BootstrapStarted / BootstrapCompleted(status: Success|NoOp, duration_ms) / BootstrapFailed; step logs are durable with durations.
- Config knobs to disable or force ("always") globally and per-VM; defaults favor "once per stamp".
- Status remains responsive during long runs; failures surface cleanly via events and exit codes.

Cross-link
- May consume ManagedImageVerificationResult (Thread 10) to validate inputs but must not block when absent.

Anchors
- docs/BOOTSTRAP.md; src/core/status.rs; state-root conventions; src/core/reporter.rs.
---


---

