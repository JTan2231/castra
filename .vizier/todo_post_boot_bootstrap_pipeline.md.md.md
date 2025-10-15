
---
Update (tests added): unix‑gated end‑to‑end coverage

Progress
- Added a unix‑gated integration test that stubs ssh/scp/qemu and exercises the bootstrap pipeline end‑to‑end: handshake gating, step sequence/events, idempotence stamps, durable run logs, and NoOp replay.

Next slice (implementation behind flag)
- Wire BootstrapStarted/Completed(NoOp|Success)/Failed events in the single‑VM path behind an opt‑in flag, persisting step logs.

Notes
- Keep behavior off by default until docs/contract (docs/BOOTSTRAP.md) lands; tests validate the contract without exposing it to users yet.
---


---

