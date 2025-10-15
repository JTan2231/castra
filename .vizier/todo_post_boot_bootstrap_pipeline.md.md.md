
Progress note
- Unix-gated integration test added exercising end-to-end bootstrap flow with stubbed ssh/scp/qemu: gating on handshake, step events/log capture, idempotence stamps, durable logs, and NoOp replay.

Next slice
- Implement BootstrapStarted/Completed(NoOp|Success) emission with durable step logs for single-VM path behind an opt-in flag; respect stamps for NoOp and ensure UI remains responsive.


---

