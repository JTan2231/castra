Update (SNAPSHOT v0.4)

Evidence
- `up` performs preflight: QEMU binary discovery, port conflict checks, port availability probes, and overlay creation via `qemu-img` when present; otherwise prints actionable manual command.
- `up` launches QEMU with user-mode NAT and declared hostfwd rules; writes per-VM pidfiles and logs (QEMU stdout/stderr and serial) under `.castra/logs/` and pidfiles under `.castra/`.
- `status` derives state from pidfile/process checks and prints UPTIME from pidfile mtime.
- `down` sends SIGTERM, waits, escalates SIGKILL, and removes pidfile; prints progress.

Refinement
- Prefer ACPI power-button guest shutdown before TERM/KILL where possible; fall back to signals.
- Split runtime directories explicitly: `.castra/run/` for pidfiles/sockets and `.castra/logs/` for logs (code currently uses `.castra` + `logs/`).
- Add QMP socket hook for richer lifecycle/health in future.

Acceptance criteria (amended v0.4)
- Current implementation satisfies MVP state detection and launch/teardown. [DONE]
- Follow-up: implement ACPI-first shutdown and adopt `.castra/run/` layout without breaking UX. [NEXT]


---

