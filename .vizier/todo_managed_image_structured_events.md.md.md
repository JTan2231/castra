---
Thread 10 — Managed images: structured verification/profile events (canonical)

Tension
- Verification/profile steps occur but lacked machine-parseable events; CLEAN and automation could not reliably consume results.

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

Update (v0.8.5)
- Shipped: Structured events for verification/profile (Started/Result), with size_bytes, duration_ms, checksums, and steps[]. Emission wired in up; app/up renders sizes, durations, and steps.
- Shipped: CLEAN links reclaimed-bytes evidence to ManagedImageVerificationResult and surfaces linkage in CLI.
- Outstanding: Ensure reporter durability across sinks (unified stream + per-image logs) via smoke tests; document field stability and provide JSON examples in docs and CLEAN.md.

Refined acceptance (remaining)
- Events are durable and visible in both unified and per-image logs under stress (concurrent VMs; long runs), verified by smoke tests.
- Docs enumerate stable fields with examples; CLEAN.md references these fields explicitly.

Verification plan (smoke)
- Launch 2+ VMs with managed images; verify both sinks receive Started/Result for verification and profile; assert field presence (image_id, path, size_bytes, duration_ms, checksums, steps).
- Run CLEAN; confirm evidence link back to the latest verification result appears in CLI and JSON.
- Repeat runs (idempotent) to ensure stability and absence of field drift.

Pointers (non-prescriptive anchors)
- src/managed/mod.rs (verification/profile flow)
- src/core/reporter.rs; src/core/logs.rs (emission/durability)
- src/core/operations/clean.rs (linkage to events)

Cross-links
- Thread 12 may consume these results to short-circuit when profile already applied and hashes match.
---
