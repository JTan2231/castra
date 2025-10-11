Thread: Networking and connectivity ergonomics (depends on SNAPSHOT v0.1)

Goal
- Make VM↔host connectivity predictable with safe defaults.

Acceptance criteria
- Default networking is QEMU user-mode NAT; works without extra host setup.
- Host port mappings can be declared in config; conflicts are detected with friendly errors and suggestions.
- `castra ports` lists all active forwards and the host broker endpoint in a stable, copyable format.
- Works on macOS and Linux hosts in default path; Windows status TBD but does not crash.

Notes
- Keep implementation open; focus on behavior and UX copy.