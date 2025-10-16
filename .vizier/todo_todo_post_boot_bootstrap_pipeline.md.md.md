
---
Update — v0.8.6 idempotence/overrides tightened

Progress
- Per‑invocation overrides ship: `castra up --bootstrap <mode>` and per‑VM forms (`--bootstrap <vm>=<mode>` / `vm:mode`) with precedence (per‑VM over global) and conflict detection.
- Tests strengthen idempotence stamps and NoOp flow: unchanged (base_image_hash, bootstrap_artifact_hash) yields BootstrapCompleted(status: NoOp) with no side effects. Forced (Always) mode runs despite a stamp and emits Success with durable logs; Disabled yields Skipped outcome and an info diagnostic.
- Handshake timeout failures produce a Failed WaitHandshake step and BootstrapFailed with durable error log. Sub‑second polling respects configured deadlines.

Remaining scope (next slice)
- Persist idempotence stamps under the state root with the finalized key shape and verify NoOp path integrates with durable step logs.
- Smoke tests for reruns on unchanged inputs and for override interactions (Disabled vs Always) across multiple VMs; confirm outcomes are returned in input order without blocking others.
- Docs: examples of event payloads and durable log layout in docs/BOOTSTRAP.md; call out override precedence and NoOp semantics.

Acceptance refinements
- Triggering exactly once per (base_image_hash, bootstrap_artifact_hash) is observable via stamps + events; NoOp has no side effects.
- Overrides honored with clear precedence; conflicts rejected with preflight error; status remains responsive; logs are durable with durations per step.

Anchors
- docs/BOOTSTRAP.md; src/core/status.rs; state‑root conventions; src/core/reporter.rs; src/app/up.rs.
---


---

Complete bootstrap stamp persistence, rerun semantics, and docs.
Persist idempotence stamps under the state root keyed by (base_image_hash, bootstrap_artifact_hash); ensure NoOp integrates with durable step logs; add smoke tests for reruns and override interactions across multiple VMs; document event payloads and durable log layout, including override precedence and NoOp semantics. (thread: post-boot-bootstrap-pipeline)

Acceptance Criteria:
- Stamps are written under the state root keyed by (base_image_hash, bootstrap_artifact_hash) and are discoverable for each VM; reading a stamp on unchanged inputs yields BootstrapCompleted(status: NoOp) with no side effects.
- Disabled and Always overrides behave as documented across multiple VMs: Disabled yields Skipped; Always forces execution despite stamps; per-VM overrides take precedence over global; conflicts are rejected preflight with a clear error.
- Rerunning with unchanged inputs produces NoOp with existing durable step logs intact; successful forced runs update logs and stamps appropriately.
- Outcomes are returned in input order without blocking other VMs; status remains responsive during waits and execution.
- Events include BootstrapStarted / BootstrapCompleted(status: Success|NoOp|Skipped) / BootstrapFailed with stable fields; step logs are durable and include durations.
- docs/BOOTSTRAP.md includes example event payloads, durable log layout, and a clear explanation of override precedence and NoOp semantics.

Pointers:
- docs/BOOTSTRAP.md
- src/core/status.rs (handshake fields)
- state-root conventions
- src/core/reporter.rs; src/app/up.rs

Implementation Notes (safety/correctness):
- Stamp writes must be atomic and only reflect Success completion; NoOp must not mutate logs or state.
- Concurrent per-VM runs must not race stamp writes; ensure a single durable failed-run log when BootstrapFailed occurs.