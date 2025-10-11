
---
Update (SNAPSHOT v0.5)

Evidence
- Default networking is QEMU user-mode NAT with hostfwd rules derived from config.
- ports lists declared forwards and flags duplicate-host-port conflicts and broker-port reservation; broker endpoint printed.
- Works on macOS/Linux (HVF/KVM flags set); Windows not targeted yet but code paths avoid Windows-specific assumptions.

Refinement
- Distinguish declared vs active forwards at runtime (e.g., via QMP) and consider `ports --active`.

Acceptance criteria (amended v0.5)
- `ports` shows declared forwards with conflict/broker status. [DONE]
- Future: optional `--active` shows live forwards for running VMs. [OPEN]


---

