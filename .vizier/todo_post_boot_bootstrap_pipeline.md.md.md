---
Progress
- Added a unix-gated integration test (`bootstrap_pipeline_runs_and_is_idempotent`) that exercises the happy-path bootstrap pipeline with stubbed ssh/scp binaries.
- The test verifies handshake gating, transfer/apply/verify steps, stamp persistence, log durability, and the NoOp replay path while asserting structured events.

Impact
- Provides executable coverage for Thread 12’s core acceptance criteria without requiring real network credentials, reducing regression risk as the pipeline evolves.

Next
- Layer focused tests around failure paths (connect/transfer/apply) once error surface wiring is finalized.
---


---
Clarifications and anchors
- Trigger: first successful broker handshake signal per VM (see src/core/status.rs: reachable, last_handshake_age_ms). Subsequent runs key off (base_image_hash, bootstrap_artifact_hash).
- Preconditions: ManagedImageVerificationResult (Thread 10) may be used to validate inputs but must not block when absent.
- UX knob sketch: global and per-VM config to disable or force ("always"); safe defaults favor "once per stamp" with clear eventing.

Next acceptance slice
- Emit BootstrapStarted/Completed(NoOp|Success) with durable step logs for connect/transfer/apply/verify on a single VM path, behind a feature flag or opt-in config.
---


---

