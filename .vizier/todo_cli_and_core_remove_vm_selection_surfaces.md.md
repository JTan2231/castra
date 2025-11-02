
---
Update (scope refinement with concrete anchors)
- In options.rs, remove or deprecate UpOptions.broker_only (bus-era) alongside any VM-selection flags (none exposed directly but verify per-VM BootstrapOverrides usage is strictly planning/hints, not runtime selection UI).
- Remove API types that imply bus routing: BrokerOptions, BusPublishOptions, BusTailOptions, BusLogTarget.
- CleanOptions.include_handshakes: drop or rename to a generic "include_handshakes (deprecated)" no-op and document removal under Thread 30.
Evidence: options.rs file shows these items present today.

---

---
Update (scope refinement with concrete anchors)
- In options.rs, remove or deprecate UpOptions.broker_only (bus-era) alongside any VM-selection flags (none exposed directly but verify per-VM BootstrapOverrides usage is strictly planning/hints, not runtime selection UI).
- Remove API types that imply bus routing: BrokerOptions, BusPublishOptions, BusTailOptions, BusLogTarget.
- CleanOptions.include_handshakes: drop or rename to a generic "include_handshakes (deprecated)" no-op and document removal under Thread 30.
Evidence: options.rs file shows these items present today.

---

