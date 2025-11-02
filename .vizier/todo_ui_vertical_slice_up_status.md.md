---
Alignment with Harness stream
- Ensure the vertical slice subscribes to the harness unified event stream, processing version + vizier context preambles, then rendering live Up status by agent.id.
- Acceptance: Starting an Up via UI shows interleaved lifecycle + vizier.ssh events without any VM selection UI; the status footer reflects agent-scoped attention.

---

