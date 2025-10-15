Thread 10 — Managed images: structured verification/profile Events (Product level)

Why
- Downstream tooling (clean, bootstrap) needs machine-parseable evidence about verification and profile application.

Desired behavior
- During managed image operations, emit structured events with stable names and fields:
  - VerificationStarted / VerificationResult: includes algorithm(s), expected vs actual checksums, sizes, durations, and outcome.
  - ProfileApplied / ProfileResult: includes profile id/name, steps executed, durations, and outcome.
- CLEAN consumes these to report reclaimed bytes with before/after evidence.

Acceptance criteria
- Events are present in per-VM or image-scoped logs and in reporting APIs.
- Fields are sufficient for automation; event names stable and documented in code comments.
- CLEAN output references these events to justify byte totals when applicable.

Anchors
- src/managed/mod.rs; src/core/logs.rs; src/core/reporter.rs; src/core/operations/clean.rs

Notes
- Leave transport/serialization details flexible as long as events are machine-parseable and durable.