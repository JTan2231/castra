Thread: QEMU backend and VM lifecycle (depends on SNAPSHOT v0.1, QEMU-only constraint)

Goal
- Start and stop a single VM via QEMU with safe defaults.

Acceptance criteria
- `castra up` boots one VM from a base image+overlay with default CPU/mem, using QEMU user-mode networking (NAT).
- `castra status` reports: stopped | starting | running | shutting_down states based on process and guest ping/serial heartbeat.
- `castra down` gracefully shuts the VM, escalating to SIGTERM → SIGKILL after timeout.
- All artifacts live in a project-local `.castra/` directory.

Notes
- Implementation detail is open (direct QEMU invocation vs. helper), but behavior must hold.
- Detect QEMU presence and fail with a friendly preflight message if missing.
