Snapshot ref bump: v0.7.2. Event ordering and visibility tightened.

- Emit Events strictly in order per-VM and ensure they are observable in both logs and OperationOutput streams.
- Timeouts: document sane defaults in help (e.g., graceful wait), while keeping values configurable via existing options surface if available.


---

