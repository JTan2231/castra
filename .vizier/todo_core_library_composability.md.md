Update — Injectable broker/helper runner

- Acceptance addition: Public API accepts a broker/helper runner configuration that can be a no-op or custom launcher when embedded; core must not hard-bind to the current executable for helper processes.
- Docs/example: Show a library example that runs Up with a no-op broker runner and still emits events (demonstrates decoupling).
- Anchors: castra-core/src/lib.rs; castra-core/src/core/broker.rs; examples/library_up.rs.


---

