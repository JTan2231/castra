Update — No-duplicate-window constraint

- Acceptance addition: Launching Up from the UI must not spawn an additional UI or CLI window. Demo checklist includes verifying only one castra-ui process remains active throughout the operation.
- Cross-link: Satisfies Thread 20 defect T20-1 (broker launch decoupling).


---

Update — Unblocked by broker decoupling

- Precondition satisfied: Broker launch is injectable in core; UI may proceed to consume event stream without spawning extra windows.
- Action: Wire controller to operations::up_with_launcher; pass a launcher sourced from CASTRA_CLI_EXECUTABLE or a custom spawn function suitable for the host platform.
- Acceptance reminder: Demo checklist must confirm single castra-ui process during Up; include process list snippet.
- Anchors add: castra-core/src/core/runtime.rs (launcher types), castra-core/src/app/up.rs (CLI-only path for reference).


---

