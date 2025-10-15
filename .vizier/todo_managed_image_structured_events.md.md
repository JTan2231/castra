---
Snapshot sync (v0.8.1)

- Keep event names per snapshot: ManagedImageVerificationStarted/Result and ManagedImageProfileApplied/Result.
- Acceptance clarifications:
  - Reporter emits via unified channel with lifecycle events; events are durable and appear in per‑VM or image‑scoped logs.
  - CLEAN output links reclaimed‑bytes evidence to ManagedImageVerificationResult entries (image id/path + timestamp) when available.
- Cross‑link Thread 12: bootstrap may use these results to short‑circuit when profile already applied and hashes match.


---

