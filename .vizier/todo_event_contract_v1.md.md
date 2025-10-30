Status update (2025-10-30):
- Contract dependency tightened: UI work now blocks on documented event shapes and golden tests. Core already emits structured JSON; we must freeze v1 and publish examples.

Acceptance additions:
- Include a minimal stream preamble event with version: {"contract":"castra-events","version":"1"} before first event, or a version field per event; document whichever is chosen and ensure tests assert it.
- Provide examples in docs/dev/consuming_event_contract.md and castra-ui/docs/reference/attention_model.md referencing the same schema.
- Add a failure-classification rubric: transient vs terminal errors, with remediation_hint optional field.

Verification notes:
- Extend castra-core/tests/broker_contract.rs with golden JSON snapshots for Up phases across multiple VMs.
- Include round-trip tests using serde_json to ensure no extra fields are required by UI.

Scope clarification:
- v1 covers Up and Status streaming; Down/Clean/Bootstrap may be documented as provisional with clear stability notes if not fully frozen.

---

