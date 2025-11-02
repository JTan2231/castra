
---
Update (evidence from options.rs)
- options.rs still defines BrokerOptions, BusPublishOptions, BusTailOptions, BusLogTarget and the UpOptions.broker_only flag; CleanOptions.include_handshakes also persists. These are bus-era artifacts and must be removed or hard-disabled with deprecation guidance.
- Acceptance addendum: Public API no longer exposes these types/flags; attempting to compile downstreams that reference them should fail with clear changelog notes. CLI path for `broker`/`bus` remains deprecating or removed (see CLI cleanup TODO).
Anchors: castra-core/src/core/options.rs.

---

---
Update (evidence from options.rs)
- options.rs still defines BrokerOptions, BusPublishOptions, BusTailOptions, BusLogTarget and the UpOptions.broker_only flag; CleanOptions.include_handshakes also persists. These are bus-era artifacts and must be removed or hard-disabled with deprecation guidance.
- Acceptance addendum: Public API no longer exposes these types/flags; attempting to compile downstreams that reference them should fail with clear changelog notes. CLI path for `broker`/`bus` remains deprecating or removed (see CLI cleanup TODO).
Anchors: castra-core/src/core/options.rs.

---

