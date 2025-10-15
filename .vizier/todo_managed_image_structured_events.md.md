---
Thread 10 — Managed images: structured verification/profile events (canonical)

Tension
- Verification/profile steps occur but lack machine-parseable events; CLEAN and automation cannot reliably consume results.

Change (product-level)
- Emit structured events around managed-image verification and profile application via the unified reporter channel.

Event names (stable)
- ManagedImageVerificationStarted { image_id, path }
- ManagedImageVerificationResult { image_id, path, checksums: { algo, value }[], size_bytes, duration_ms, outcome: Success | Failure, error? }
- ManagedImageProfileApplied { image_id, profile_id, steps: string[] }
- ManagedImageProfileResult { image_id, profile_id, duration_ms, outcome: Success | Failure | NoOp, error? }

Acceptance criteria
- Events appear in per-image logs and unified streams alongside lifecycle events.
- CLEAN command can, when available, link reclaimed-bytes evidence to a prior ManagedImageVerificationResult (by image_id/path + timestamp proximity) and surface that linkage in output.
- Fields are stable and JSON-safe to support downstream tooling.

Pointers (non-prescriptive anchors)
- src/managed/mod.rs (verification/profile flow)
- src/core/reporter.rs; src/core/logs.rs (emission/durability)
- src/core/operations/clean.rs (linkage to events)

Cross-links
- Thread 12 may consume these results to short-circuit when profile already applied and hashes match.
---
