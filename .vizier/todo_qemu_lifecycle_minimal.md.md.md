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

