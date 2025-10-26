
---
Update (re: CORE.md broker launch coupling)

Problem statement (evidence): When core operations are invoked from castra-ui, start_broker resolves std::env::current_exe() to the UI binary and re-executes it with the `broker` subcommand, causing a duplicate UI window and breaking embedding. This couples castra-core to the CLI binary identity.

New acceptance (must-haves to claim composability):
- Core does not assume `current_exe()` is the CLI. Broker (and any helper) launch is injectable/configurable by embedders and the CLI, with a deterministic default for CLI usage.
- Library callers can run Up/Down/Status/Clean/Bootstrap without spawning any extra UI/CLI windows or processes unless explicitly requested via options.
- A documented embedding surface (options or trait/hook) for supplying a broker runner; default CLI path documented. Deprecated `current_exe()` path.
- Tests: embedding example runs Up and captures events without spawning the CLI; golden test ensures no `castra-ui broker` process is spawned when embedding.

Scope adjustments:
- Audit all subprocess spawns (broker, bootstrap helpers, bus ops) for `current_exe()`/CLI assumptions and move them behind the same injectable surface.

Anchors: castra-core/src/core/runtime.rs; castra-core/src/core/operations/bus.rs; castra-core/src/core/broker.rs; castra-core/src/lib.rs; CORE.md.

Thread link: Thread 20 (UI ↔ Core integration).

---

