
---
Delta (align with current snapshot)
- Add bounded, opportunistic orphan reclamation pass triggered on the next `status`, `clean`, or `up`, without blocking healthy operations.
- CLEAN output explicitly attributes reclaimed bytes to ephemeral layers by VM/session when evidence is available.
- Non-JSON TTY includes a concise ephemerality reminder with a pointer to docs; avoid repeating across Up/Down excessively.

Acceptance additions
- After abnormal termination, the next command performs reclamation within bounded time and reports reclaimed bytes under CLEAN; Up/Down TTY shows the ephemerality notice once per VM per session.
---

---

