
---
Update (agent-addressed fields and deprecations)
- Each event MUST include agent.id; MAY include agent.role and groups[].
- Remove any vm_id field from public contract docs; if present internally, treat as legacy and do not expose to consumers.
- Add examples for vizier.ssh.connected, vizier.ssh.output, and lifecycle events with agent.* fields. Acceptance: UI/tests can filter by agent.id without any VM selection surface.
Anchors: castra-core/docs/event-contract-v1.md; castra-harness/src/events.rs; castra-ui/docs/dev/consuming_event_contract.md.

---

---
Update (agent-addressed fields and deprecations)
- Each event MUST include agent.id; MAY include agent.role and groups[].
- Remove any vm_id field from public contract docs; if present internally, treat as legacy and do not expose to consumers.
- Add examples for vizier.ssh.connected, vizier.ssh.output, and lifecycle events with agent.* fields. Acceptance: UI/tests can filter by agent.id without any VM selection surface.
Anchors: castra-core/docs/event-contract-v1.md; castra-harness/src/events.rs; castra-ui/docs/dev/consuming_event_contract.md.

---
Pivot — remove bus/broker families; add vizier.ssh + preamble
- Explicitly exclude any broker/bus event families from v1. Introduce vizier.ssh.* events (connecting, connected, stdout, stderr, failed{remediation_hint}, disconnected).
- Require a version preamble at stream start, followed by an optional vizier operational context preamble (from harness PromptBuilder).
- Acceptance: Examples include unified stream samples showing version + vizier context, then interleaved core lifecycle and vizier.ssh events. Golden tests relocate to harness.

---

