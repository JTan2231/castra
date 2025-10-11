---
Update (SNAPSHOT v0.7)

Evidence
- Broker subcommand binds 127.0.0.1:<port>, writes pidfile, logs timestamped lines, greets clients; `up` ensures idempotent start; `status` reports listening/offline. Logs prefixed as [host-broker].

Next
- Minimal guest handshake to flip BROKER column to "reachable" when a VM connects; keep copy consistent with status legend.


---

