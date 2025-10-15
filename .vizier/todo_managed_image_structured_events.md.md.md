
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
- CLEAN command now links reclaimed managed bytes to the latest ManagedImageVerificationResult per image, surfacing root-disk paths, byte totals, and verification/filesystem delta in CLI output.

Remaining gaps (acceptance deltas)
- Ensure durability/visibility: verify these events flow through all reporters/log sinks (unified channel already used; need per-image log scoping docs/smoke test).
- Document field stability in docs/library_usage.md and CLEAN.md; include examples with the new plan/report/steps fields.

Acceptance tweaks
- Keep duration_ms and size_bytes required on Result events (met).
- Require steps[] to be present (possibly empty) on Profile* events to allow deterministic parsing (now met in emission path).

Next steps (Thread 10)
- Extend reporter/JSON docs with sample ManagedImage events and CLEAN evidence payload; verify non-CLI reporters persist managed evidence details.
- Add a minimal JSON example to docs showing the four events and their fields.
---


---

---
Update — v0.8.5 shipped CLEAN linkage and profile steps; remaining work scoped

Shipped
- Event variants and fields (Started/Result for Verification and Profile) are emitted with duration_ms and size_bytes where applicable.
- app/up.rs renders steps/durations/sizes.
- CLEAN now links reclaimed-bytes evidence to the latest ManagedImageVerificationResult per image and surfaces linkage in CLI output.

Remaining acceptance (narrowed)
- Reporter durability: verify events appear in both unified streams and per-image logs across all configured sinks; add smoke tests.
- Documentation: add stable-field references and JSON examples to docs/library_usage.md and CLEAN.md covering plan/report/steps and CLEAN linkage.

Acceptance verification
- Add a smoke test that runs `castra up` with a managed image and captures JSON logs to assert presence of the four ManagedImage* events with required fields, then runs `castra clean` and asserts evidence linkage appears with matching image id/path.
---


---

