---
Update (Snapshot v0.8.0 alignment)

- Clarify event names to match snapshot language and stress durability:
  - VerificationStarted → ManagedImageVerificationStarted
  - VerificationResult → ManagedImageVerificationResult
  - ProfileApplied → ManagedImageProfileApplied
  - ProfileResult → ManagedImageProfileResult
- Add requirement that CLEAN links reclaimed bytes to specific ManagedImageVerificationResult entries (by image id/path + timestamp) in output when available.
- Acceptance addition: reporter emits these via the same channel used by lifecycle events so tools can consume a unified stream.
- Cross-link: Thread 12 (bootstrap) may read these results to skip bootstrap when profile already applied and hashes match.

Anchors unchanged.

---

