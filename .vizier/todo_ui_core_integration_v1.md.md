Update — Broker launch abstraction and embedding safety

- Add acceptance: Broker/helper launch is injectable/configurable; must not rely on std::env::current_exe() that binds to the embedding process. Default remains deterministic for CLI usage.
- Verification: During UI-initiated Up, no additional castra-ui (or duplicate window) is spawned. Capture a process list sample in the demo notes and assert in an automated smoke where feasible.
- Anchors: castra-core/src/core/broker.rs; castra-core/src/core/operations/bus.rs; castra-core/src/core/runtime.rs; CORE.md (defect described).
- Risk notes: Audit any other subprocess invocations for similar assumptions.


---

Update — Abstraction landed; UI wiring next

- Status: BrokerLauncher abstraction is implemented in core (runtime.rs). start_broker() requires a launcher; tests verify injected usage.
- Next acceptance slice: castra-ui initiates Up by calling operations::up_with_launcher with an injected launcher.
- UX constraint (unchanged): No duplicate UI/CLI window when launching Up from the UI.
- Integration notes: For embedding, prefer ProcessBrokerLauncher::from_env or a custom launcher. Document and respect CASTRA_CLI_EXECUTABLE when present.
- Evidence: runtime.rs (ProcessBrokerLauncher, BrokerLauncher), app/up.rs (CLI-specific resolve_cli_executable isolated).


---

