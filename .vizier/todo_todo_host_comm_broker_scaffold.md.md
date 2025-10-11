Update (SNAPSHOT v0.4)

Evidence
- Host broker implemented as a hidden `broker` subcommand; `up` spawns it with pidfile/logfile, binding 127.0.0.1 on configured port (default 7070).
- `status` reports broker process state (listening/offline) and includes endpoint.
- `logs` tails broker log with `[host-broker]` prefix.

Refinement
- Implement a simple VM handshake path to mark BROKER as `reachable` when a guest client connects (status currently shows `waiting` vs `offline` only).
- Provide a discoverable hint to VMs (e.g., via serial log banner or a config file in a shared folder) with broker endpoint; choice left open.

Acceptance criteria (amended)
- `status` reflects `reachable` once a guest has performed a basic TCP handshake to the broker. [NEXT]


---

