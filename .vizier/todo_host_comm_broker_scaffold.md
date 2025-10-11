Thread: Host communication channel (depends on SNAPSHOT v0.1)

Goal
- Provide a host-side broker scaffold that agents can reach; VMs can connect via NAT to host ports.

Acceptance criteria
- `castra up` starts a lightweight host listener on a configurable TCP port (default 7070) and prints its address in `status`.
- VMs receive the host address via a simple environment file or serial line hint (mechanism can be stubbed initially).
- `ports` displays the broker port and any VM forwarded ports.

Notes
- Protocol left open; start with a no-op echo/health endpoint to validate connectivity.
---
Update (SNAPSHOT v0.2)

Evidence
- No broker yet; `ports`/`status` exist as NYI surfaces.

Refinement
- Reserve default broker port (7070) in config defaults or internal constants; show it in `ports` even when broker is stubbed to set UX expectations.
- Provide a no-op health endpoint and log a single line on connection to validate observability thread.

Acceptance criteria (clarified)
- `castra status` shows broker endpoint once `up` is run, even if VMs are not yet connected.

---

