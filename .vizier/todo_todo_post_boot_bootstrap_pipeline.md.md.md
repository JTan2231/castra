

---
Progress update (v0.8.6)
- Pipeline runs per-VM concurrently with live event streaming while preserving per-VM ordering.
- Handshake wait uses sub-second slices and on timeout emits a failed WaitHandshake step and BootstrapFailed; a single failed run log is persisted with error detail.
- Outcomes returned in original VM order; first error captured without blocking others.
- CLI supports per-invocation overrides for bootstrap mode (global and per-VM) with conflict detection and precedence (per-VM over global). docs/BOOTSTRAP.md updated; unit tests cover parsing/precedence.

Next slice
- Persist idempotence stamps under state root keyed by (base_image_hash, bootstrap_artifact_hash) and emit BootstrapCompleted(status: NoOp) on unchanged inputs.
- Add smoke tests for reruns and for override interactions with disable/force knobs.
Acceptance addendum
- Safe re-runs: when inputs unchanged, emit NoOp without side effects; when forced, re-run regardless of stamp.
---

---

