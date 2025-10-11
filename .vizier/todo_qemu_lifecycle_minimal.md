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
---
Update (SNAPSHOT v0.2)

Evidence
- CLI scaffolding for `up`/`down`/`status` exists; behavior currently NYI with structured errors.

Refinement
- First pass targets single VM only, driven by fields already present in generated castra.toml (base_image, overlay, cpus, memory, port_forwards).
- Include a preflight for QEMU presence and overlay existence/creation, coordinating with the Safety/Preflight thread.

Acceptance criteria (clarified)
- `castra up` creates overlay if missing (or explains how to initialize), then boots VM with user-mode NAT and declared forwards.
- `status` derives running state from process handle/pidfile presence for MVP (heartbeat optional in v1).
- `down` sends ACPI shutdown and escalates on timeout.

---

Thread: QEMU backend and VM lifecycle (depends on SNAPSHOT v0.3, QEMU-only constraint)

Goal
- Start and stop a single VM via QEMU with safe defaults.

Acceptance criteria
- `castra up` boots one VM from a base image+overlay with default CPU/mem, using QEMU user-mode networking (NAT).
- `castra status` reports: stopped | starting | running | shutting_down states based on pidfile/process checks (heartbeat optional v1).
- `castra down` gracefully shuts the VM, escalating to SIGTERM → SIGKILL after timeout.
- All artifacts live under `.castra/` (images and runtime separated).

Notes
- Implementation detail is open (direct QEMU invocation vs helper), but behavior must hold.
- Detect QEMU presence and fail with a friendly preflight message if missing.

---
Update (SNAPSHOT v0.3)

Evidence
- CLI surfaces exist; `ports` will feed planned forwards into QEMU invocation once implemented.

Refinement
- Minimum viable `status` derives running from pidfile/process presence; broker/heartbeat is additive.
- Coordinate with Safety/Preflight to check: qemu-system binary present, CPU/mem within host limits, overlay/image paths writable.
- Store runtime artifacts in `.castra/run/` (pidfiles, qmp sockets, serial logs) and images in `.castra/images/` (overlay), aligning with Storage Hygiene thread.

Acceptance criteria (clarified v0.3)
- `up`: create overlay if missing; launch QEMU with user-mode NAT and declared forwards; write pidfile; capture serial to log.
- `status`: stopped/starting/running/shutting_down via pidfile + process checks; uptime derived from process start.
- `down`: ACPI shutdown, wait with timeout, escalate TERM→KILL; remove pidfile on success.
- Preflight failure produces friendly, actionable error without partial side effects.
---