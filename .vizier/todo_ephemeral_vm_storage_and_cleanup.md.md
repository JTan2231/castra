
---
Change directive (2025-10-16): Bootstrap stamps removed project-wide.

Scope adjustment
- Replace references to “host‑durable stamps” with “host‑durable logs/events.” Ephemeral/persistent disk behavior remains unchanged; no bootstrap stamp artifacts are expected or maintained.

Acceptance update
- Persistence settings do not interact with any bootstrap stamp mechanism (none exists). Reproducibility derives from base images plus runner idempotence, not stamps.
---

---

