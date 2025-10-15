
Progress note
- Unix-gated integration test added exercising end-to-end bootstrap flow with stubbed ssh/scp/qemu: gating on handshake, step events/log capture, idempotence stamps, durable logs, and NoOp replay.

Next slice
- Implement BootstrapStarted/Completed(NoOp|Success) emission with durable step logs for single-VM path behind an opt-in flag; respect stamps for NoOp and ensure UI remains responsive.


---


---
Progress update (v0.8.5+)
- Handshake wait failures are now observable and durable: emit BootstrapStep(WaitHandshake, Failed) followed by BootstrapFailed, and persist a single failed run log containing the timeout/error detail. Unix-gated test forces 0s handshake timeout and asserts the sequence and durability.

Next slice (unchanged)
- Behind an opt-in flag, implement the single-VM happy path emitting BootstrapStarted → durable step logs (connect, transfer, apply, verify) → BootstrapCompleted(Success|NoOp), respecting idempotence stamps and keeping status responsive.

Acceptance clarifications
- Failure visibility is part of acceptance: handshake failure must surface as a Failed step + BootstrapFailed with durable run log; re-runs with unchanged inputs stay NoOp.
---


---

