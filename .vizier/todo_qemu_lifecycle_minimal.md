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

