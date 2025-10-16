
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

