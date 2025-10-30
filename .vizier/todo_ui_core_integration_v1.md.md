Status update (2025-10-30):
- Broker launcher is now injectable in castra-core; UI no longer needs to rely on current_exe(). Use operations::up_with_launcher and provide a launcher sourced from CASTRA_CLI_EXECUTABLE or an in-process implementation.

Next steps (product-level, acceptance-driven):
- Controller wiring: castra-ui/src/controller/command.rs triggers Up via castra-core library surface, passing an event subscriber that forwards JSON events into UI state.
- Live rendering: components/message_log.rs, components/vm_fleet.rs, components/status_footer.rs reflect incoming events in near-real time; no additional CLI/terminal windows are spawned.
- Error path: if broker launch fails (launcher returns error), surface a single, actionable error in the message log with remediation hint; UI remains responsive.
- Workspace selection: respect the current UI model for selecting a workspace; Up uses that context.

Acceptance criteria (observable):
- From the UI: initiating Up shows VM entries appearing with phase/progress updates, footer shows aggregate status, and message log streams events. On completion, a clear success/summary banner appears.
- No duplicate windows/process UIs are opened during the run.
- Disconnected mode: when the contract stream ends, UI shows a finished state; on error, shows a recoverable error and allows retry.

Anchors refined: castra-core/src/core/runtime.rs (BrokerLauncher), castra-core/src/app/up.rs (operations::up_with_launcher), castra-ui/src/controller/command.rs, castra-ui/src/components/*, env var CASTRA_CLI_EXECUTABLE.

---

