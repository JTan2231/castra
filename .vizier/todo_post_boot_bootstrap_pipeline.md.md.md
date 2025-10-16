

---
Progress update (v0.8.6)
- Per-VM concurrent runs with live event streaming and ordered per-VM events are shipped; outcomes returned in input order with first-error capture.
- Handshake waiting uses sub-second polling slices respecting configured deadlines; on timeout, a Failed WaitHandshake step is recorded, BootstrapFailed is emitted, and a single durable failure log is persisted with error detail.
- Per-invocation CLI overrides implemented: `castra up --bootstrap <mode>` (global) and per-VM forms (`--bootstrap <vm>=<mode>` / `vm:mode`); precedence is per-VM over global; conflicting overrides fail preflight. docs/BOOTSTRAP.md documents invocation, event contract, and durable logs. Unit tests cover parsing and precedence.

Next slice
- Finalize idempotence stamps and NoOp flow: persist stamps under state root keyed by (base_image_hash, bootstrap_artifact_hash); on unchanged inputs, emit BootstrapCompleted(status: NoOp) without side effects. Add smoke tests for reruns and for interactions between overrides and disable/force knobs.

Acceptance refinements
- Overrides honored with clear precedence; conflicts rejected with actionable errors.
- Safe re-runs produce NoOp with durable log note and no side effects.
---

---



Progress update (v0.8.6)
- Per-invocation bootstrap overrides shipped: global (`castra up --bootstrap <mode>`) and per-VM (`--bootstrap <vm>=<mode>` / `vm:mode`), with conflict detection and per-VM precedence. docs/BOOTSTRAP.md updated; unit tests cover parsing and precedence.
- Handshake timeout failure path implemented with durable step logs and BootstrapFailed; sub-second polling respects configured deadlines.
- Per-VM concurrent execution with live event streaming; outcomes returned in input order; first error captured without blocking others.

Next slice
- Finalize idempotence stamps under state root keyed by (base_image_hash, bootstrap_artifact_hash). On unchanged inputs, emit BootstrapCompleted(status: NoOp) without side effects. Add smoke tests for reruns and override interactions with disable/force.

Acceptance clarifications
- Safe re-runs must emit NoOp with zero side effects when stamps unchanged. Overrides must maintain precedence (per-VM over global) and reject conflicts with clear preflight errors.


---

