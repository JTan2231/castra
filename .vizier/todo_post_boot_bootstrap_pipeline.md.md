---
Thread 12 — Post-boot bootstrap pipeline (canonical)

Tension
- Users want day-1 configuration automatically after a VM is reachable; doing this manually is slow and error-prone.

Change (product-level)
- After the first successful broker handshake per VM for a given (base image hash, bootstrap artifact hash), apply a host-provided bootstrap (e.g., Nix flake) idempotently over SSH.

Trigger and idempotence
- Triggered exactly once per VM when the idempotence stamp changes: (base_image_hash, bootstrap_artifact_hash).
- Safe re-runs emit NoOp when inputs unchanged; no side effects.

Events (stable)
- BootstrapStarted { vm_id, base_image_hash, bootstrap_hash }
- BootstrapCompleted { vm_id, status: Success | NoOp, duration_ms }
- BootstrapFailed { vm_id, error }
- Durable step logs for: connect, transfer, apply, verify (with durations)

Acceptance criteria
- Config knobs to disable or force ("always") globally and per-VM with safe defaults.
- Status/UI remain responsive during long runs; progress can be observed via events/logs.
- Portability target: macOS/Linux hosts with POSIX + Nix + SSH; failure modes are reported cleanly via events and exit codes.

Pointers (non-prescriptive anchors)
- docs/BOOTSTRAP.md (contract)
- src/core/status.rs (handshake signals)
- state-root conventions for idempotence stamps
- src/core/reporter.rs (events)

Cross-links
- Consumes ManagedImageVerificationResult (Thread 10) when present to validate inputs.
---
