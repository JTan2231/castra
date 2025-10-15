
---
Refinement (denial/timeout paths + examples)
- Deterministic logs/events for handshake denial and timeout paths mirroring success path semantics (reason/action/detail fields).
- Tests:
  - Denial path: broker refuses handshake; status.reachable=false; last_handshake_age_ms updated; emits WARN with reason=denied.
  - Timeout path: no response within deadline; emits heartbeat/handshake timeout records and cleans up session deterministically.
- Docs: status JSON examples for denial/timeout; logs snippets; BUS.md linkage where relevant.
- Non-blocking guarantee: status call must not hang; report last known evidence.
Cross-links: Thread 3 in snapshot (Broker reachability).

---

