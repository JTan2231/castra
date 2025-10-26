Update — Broker launch abstraction and embedding safety

- Add acceptance: Broker/helper launch is injectable/configurable; must not rely on std::env::current_exe() that binds to the embedding process. Default remains deterministic for CLI usage.
- Verification: During UI-initiated Up, no additional castra-ui (or duplicate window) is spawned. Capture a process list sample in the demo notes and assert in an automated smoke where feasible.
- Anchors: castra-core/src/core/broker.rs; castra-core/src/core/operations/bus.rs; castra-core/src/core/runtime.rs; CORE.md (defect described).
- Risk notes: Audit any other subprocess invocations for similar assumptions.


---

