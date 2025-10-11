---
Update (SNAPSHOT v0.3)

Evidence
- Config parser validates VM definitions, resolves relative paths, and exposes warnings; `ports` command can surface conflicts ahead of runtime.
- `up`/`down`/`status` handlers exist and return NYI with correct tracking hints.

Refinement
- Preflight must run before `up`: check QEMU presence (`qemu-system-*`), verify base_image exists; create overlay path parent dirs; detect host port conflicts using existing `ProjectConfig::port_conflicts()` and fail with actionable guidance.
- MVP status can rely on pidfile/process presence; reserve guest heartbeat for later.

Acceptance criteria (tightened)
- `castra up` with missing base image or absent QEMU yields preflight errors with next-steps (e.g., `brew install qemu` on macOS, `apt install qemu-system` on Debian/Ubuntu).
- Overlay is created if missing via workflow init guidance or built-in helper; on failure, output shows the intended path and a suggested command.
- `down` sends ACPI first; after N seconds without exit, escalate and report what happened.


---

