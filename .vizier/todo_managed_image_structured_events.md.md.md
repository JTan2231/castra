---
Thread 10 — Managed images: structured verification/profile events (canonical)

Tension
- Verification and profile steps occur today but lack machine-parseable, durable events. Downstream automation and CLEAN cannot reliably attribute actions/results.

Change (product-level)
- Emit explicit, type-stable events for verification and profile application, carried over the unified reporter channel and persisted to per-image logs.

Events (stable names/fields)
- ManagedImageVerificationStarted { image_id | path, checks_expected?, profile_name?, started_at? }
- ManagedImageVerificationResult { image_id | path, checksum_actual, checksum_expected?, size_bytes?, duration_ms, outcome: Success | Mismatch | Error, error? }
- ManagedImageProfileApplied { image_id | path, profile_name, started_at? }
- ManagedImageProfileResult { image_id | path, profile_name, duration_ms, outcome: Success | Skipped | Error, error? }

Acceptance criteria
- Events are explicit enums in src/core/events.rs (type-stable across reporters).
- Reporter emits via the same unified channel as lifecycle events; events are durable and appear in per-image or image-scoped logs.
- Result events include duration_ms to enable ordering independent of wall clock.
- CLEAN links reclaimed-bytes evidence to corresponding ManagedImageVerificationResult entries using image id/path and timestamp proximity; absence of evidence is explicitly indicated in `castra clean` output.
- Fields and names are stable; JSON output preserves these exact keys.

Pointers (anchors, non-prescriptive)
- src/managed/mod.rs (verification/profile surfaces)
- src/core/events.rs (event variants)
- src/core/reporter.rs (emission path)
- src/core/logs.rs (durability)
- src/core/operations/clean.rs; src/app/clean.rs; CLEAN.md (evidence linkage)

Notes
- Keep implementation flexible on how checksums/sizes are computed; the observable contract is the event schema and durability.
---

---

