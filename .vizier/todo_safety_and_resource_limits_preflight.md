Thread: Safety and resource limits (depends on SNAPSHOT v0.1)

Goal
- Prevent host overload and unsafe operations.

Acceptance criteria
- Preflight check runs before `up`: verifies QEMU presence, host free disk space, CPU/mem headroom, and port conflicts.
- Sensible defaults for CPU/mem per VM; enforce ceilings unless overridden with `--force`.
- Graceful shutdown on SIGINT/SIGTERM with clear user messaging.

Notes
- Keep technical strategy open; codify the user-visible checks and messages.