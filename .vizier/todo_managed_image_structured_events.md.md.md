
---
Update — Implementation landed in core/events.rs, app/up.rs, and core/operations::up

Observed state
- Explicit event variants are now defined and emitted:
  - ManagedImageVerificationStarted { image_id, image_version, image_path, started_at, plan: ManagedImageArtifactPlan[] }
  - ManagedImageVerificationResult { image_id, image_version, image_path, completed_at, duration_ms, outcome, size_bytes, artifacts: ManagedImageArtifactReport[], error? }
  - ManagedImageProfileApplied { image_id, image_version, vm, profile_id, started_at, components, steps: string[] }
  - ManagedImageProfileResult { image_id, image_version, vm, profile_id, completed_at, duration_ms, outcome, components, steps: string[], error? }
- Supporting types added: ManagedImageArtifactPlan, ManagedImageArtifactReport, ManagedImageChecksum; profile “steps” captured for observability.
- Emission path wired in operations::up; CLI rendering updated in app/up.rs with size/duration formatting.

Remaining gaps (acceptance deltas)
- CLEAN linkage not yet surfaced: `castra clean` should correlate reclaimed bytes to ManagedImageVerificationResult by (image_id/path + time proximity) and include succinct evidence in output.
- Ensure durability/visibility: verify these events flow through all reporters/log sinks (unified channel already used; need per-image log scoping docs/smoke test).
- Document field stability in docs/library_usage.md and CLEAN.md; include examples with the new plan/report/steps fields.

Acceptance tweaks
- Keep duration_ms and size_bytes required on Result events (met).
- Require steps[] to be present (possibly empty) on Profile* events to allow deterministic parsing (now met in emission path).

Next steps (Thread 10)
- Wire CLEAN evidence linkage and output phrasing; add an acceptance check in app/clean.rs rendering.
- Add a minimal JSON example to docs showing the four events and their fields.
---


---

