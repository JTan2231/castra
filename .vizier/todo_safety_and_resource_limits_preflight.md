Thread: Safety and resource limits (depends on SNAPSHOT v0.1)

Goal
- Prevent host overload and unsafe operations.

Acceptance criteria
- Preflight check runs before `up`: verifies QEMU presence, host free disk space, CPU/mem headroom, and port conflicts.
- Sensible defaults for CPU/mem per VM; enforce ceilings unless overridden with `--force`.
- Graceful shutdown on SIGINT/SIGTERM with clear user messaging.

Notes
- Keep technical strategy open; codify the user-visible checks and messages.---
Update (SNAPSHOT v0.2)

Evidence
- No preflight checks yet; errors surface during command handling as NYI.

Refinement
- Coordinate with `QEMU lifecycle` to run preflight before `up`.
- Add friendly messages for missing QEMU binaries and port conflicts using the declared `port_forwards` in config.

Acceptance criteria (clarified)
- Running `castra up` without QEMU installed yields a clear preflight failure with install hints for macOS/Linux.

---

