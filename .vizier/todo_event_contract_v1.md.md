Update — Prioritize docs + golden tests for UI slice

- Priority raised (blocks UI wiring). Add explicit note to emit a stable version field at stream start and per-event fallback.
- Add acceptance: Provide a minimal end-to-end golden capturing Up event flow for 1 VM (OverlayPrepared → VmLaunched → Bootstrap* → summary) and assert field stability.
- Anchors add: castra-core/docs/event-contract-v1.md; castra-ui/docs/dev/consuming_event_contract.md.


---

