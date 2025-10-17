---
Correction (align with stamp-free policy)

- Remove mention of host-side idempotence stamps as a persist-at-rest artifact. Bootstrap idempotence is not managed by host stamps.
- Persisted host artifacts are limited to logs/events and durable diagnostics; no stamps or persistence toggles exist for guest disks.
- Acceptance and scope remain the same otherwise; CLEAN integration and UX notices unchanged.
---

---

