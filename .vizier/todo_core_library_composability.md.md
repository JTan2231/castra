Update — Injectable broker/helper runner

- Acceptance addition: Public API accepts a broker/helper runner configuration that can be a no-op or custom launcher when embedded; core must not hard-bind to the current executable for helper processes.
- Docs/example: Show a library example that runs Up with a no-op broker runner and still emits events (demonstrates decoupling).
- Anchors: castra-core/src/lib.rs; castra-core/src/core/broker.rs; examples/library_up.rs.


---

Update — Landed in core

- Status: Public API now accepts a BrokerLauncher; ProcessBrokerLauncher provided. Library no longer hard-binds to current_exe(); CLI path keeps deterministic spawn.
- Example follow-up: Ensure examples/library_up.rs showcases injecting a no-op/custom launcher and still receiving events. Add a brief note about CASTRA_CLI_EXECUTABLE.
- Acceptance: Mark composability criterion as met; track only example/docs work.


---

