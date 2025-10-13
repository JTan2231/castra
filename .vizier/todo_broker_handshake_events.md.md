
---
Status update (post-commit 14b41ba)
- Deterministic handshake success log implemented: includes vm, normalized capabilities, and session outcome. Persisted in handshake JSON. Legend/docs updated.
- Structured Event route present for handshake evidence.

Remaining follow-ups (narrowed scope)
- Add a negative-path test for denial cases (session_outcome=denied with reason) to ensure log/Event stability in failures.
- Docs: add a short example of the handshake log line and Event payload in README/docs with a versioned snippet, and note parser stability expectations.
- Ensure `castra status --help` references handshake evidence alongside `reachable`/`last_handshake_age_ms` semantics.

Acceptance delta
- Consider acceptance met for success-path evidence and legend; acceptance remains open only for denial-path test coverage and docs examples.

Thread link: broker-reachability — snapshot v0.7.9.

---

