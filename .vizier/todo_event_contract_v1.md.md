Update (Agent-first pivot) — Thread 31 alignment

- Replace per-VM selection expectations with agent-addressing: each event MUST include agent.id; SHOULD include agent.role/group when applicable.
- Remove any requirement that consumers maintain selectable VM lists at runtime; consumers MAY present agent attention only.
- vizier.ssh* events remain as transport-specific observations but are addressed to agent.id rather than VM identity.
- Acceptance linkage: Harness stream merges core + agent events; no per-VM routing implied.


---

