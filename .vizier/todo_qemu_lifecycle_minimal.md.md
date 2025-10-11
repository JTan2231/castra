
---
Update (SNAPSHOT v0.5)

Evidence
- up launches VMs with user-mode NAT and hostfwd; writes pidfiles; logs QEMU stdout/err and serial to files; overlays created via qemu-img if missing.
- status derives running from pidfile/process; uptime from pidfile mtime.
- down sends SIGTERM then escalates SIGKILL; removes pidfiles; broker stopped similarly. ACPI path not yet present.

Refinement
- Add ACPI-first shutdown before TERM/KILL to pursue guest-cooperative stop.
- Consider QMP socket for richer lifecycle in future (blocked behind broker/handshake work to avoid overreach now).

Acceptance criteria (amended v0.5)
- `down` attempts ACPI shutdown first; fall back to TERM→KILL on timeout. [OPEN]
- Existing MVP behaviors remain stable (pidfiles, logs, NAT forwards). [DONE]


---

---
Update (SNAPSHOT v0.7)

Evidence
- Up/Down/Status implemented across app modules; pidfiles and logs under .castra. Overlays created via qemu-img with base-format detection when available.
- Accelerator detection via `qemu-system -accel help`; adds `-accel hvf` on macOS or `-accel kvm` on Linux; sets `-cpu host` when accel engaged. NAT + hostfwd wired. Managed images resolved before launch.

Refinement
- Add ACPI-first shutdown path before TERM→KILL; consider QMP sockets later for richer lifecycle.

Acceptance status
- MVP lifecycle behaviors: DONE.
- ACPI-first shutdown: OPEN.


---

