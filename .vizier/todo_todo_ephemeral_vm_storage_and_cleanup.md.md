
---
Refinement (orphan reclamation + UX)
- On next command after an unclean exit (crash/reboot), perform bounded orphan reclamation of ephemeral layers and temp dirs without blocking healthy operations.
- CLEAN output attributes reclaimed bytes to ephemeral layers explicitly when present, independent of managed-image verification evidence.

Additional Acceptance
- Up/Down TTY includes a brief, de-duplicated reminder that guest changes are ephemeral, with a pointer to docs for exporting via SSH.
- Orphan reclamation reports a concise summary line and is reflected in CLEAN's reclaimed-bytes totals.
---

---

