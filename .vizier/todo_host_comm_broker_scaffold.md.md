
---
Update (SNAPSHOT v0.5)

Evidence
- Hidden broker subcommand binds 127.0.0.1:<port>, writes broker.pid, logs to .castra/logs/broker.log, and greets connections with a single line. up ensures broker is running (idempotent). status shows broker process listening vs offline; per-VM BROKER column shows waiting/offline.

Refinement
- Add minimal guest handshake so status can show reachable when at least one VM connects.
- Ensure broker port collisions handled before launch (already covered by preflight) and clearly communicated.

Acceptance criteria (amended v0.5)
- status reflects reachable when guest handshake implemented. [OPEN]
- Broker logs one line per connection with timestamp and level. [DONE]


---

