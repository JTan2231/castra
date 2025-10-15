Updates (clarifications)
- Acceptance expands: Events must flow through src/core/events.rs variants to ensure type-stable emission across reporters.
- Anchors added: src/app/clean.rs (surface linkage evidence to users) and CLEAN.md (narrative for reclaimed-bytes reporting).

Acceptance additions
- Human-facing `castra clean` output surfaces (when available) the linked ManagedImageVerificationResult evidence in a concise line item per image (e.g., shows image_id/path and size_bytes reclaimed); absence of evidence is still a valid, clearly indicated case.
- Event payloads include monotonic step timestamps or durations sufficient for ordering without wall-clock; at minimum, duration_ms per Result events is required and tested.

Trade space (left open)
- Internal checksum algorithms and profiling step taxonomy are not prescribed so long as declared in the event payloads and fields remain stable.


---

