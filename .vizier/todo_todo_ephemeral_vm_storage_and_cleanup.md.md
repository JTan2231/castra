---
Acceptance Criteria (explicit)
- Up/Down: TTY and JSON surfaces clearly signal that guest disks are ephemeral-only; guidance to export via SSH appears at least once per run in TTY, and a machine-readable hint is included in JSON summaries.
- Validation: Any attempt to enable persistence via CLI/config fails preflight with an error that points to SSH export instructions; no hidden env toggles.
- Cleanup: On normal shutdown, ephemeral layers and per-VM temp dirs are removed deterministically. After crash/reboot, the next Castra command triggers bounded orphan reclamation without blocking healthy operations.
- CLEAN: Reports reclaimed bytes from ephemeral layers with per-VM attribution when available; JSON schema remains stable and coexists with managed-image evidence.
- Safety: No user data loss beyond the documented ephemerality policy; logs/events remain the only durable host artifacts.

Pointers
- src/core/operations/up.rs; src/app/up.rs; src/core/operations/clean.rs; src/app/clean.rs; src/core/runtime.rs; src/app/down.rs; docs/BOOTSTRAP.md; CLEAN.md; CLI help.
---

---

