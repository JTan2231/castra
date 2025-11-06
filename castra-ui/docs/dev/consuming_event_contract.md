# Developer Notes: Consuming Event Contract v1

**Assumed versions:** Castra snapshot v0.10.0-pre · Castra UI 0.1.0 · Event Contract v1 · Attention Model draft v0.1  
**Updated:** 2024-06-02

## What Changed Since Last Version
- Documented the shift to direct SSH session metadata provided by the harness/UI (no intermediate broker).

## Contract Recap
- Canonical definitions live in [`castra-core/docs/event-contract-v1.md`](../../../castra-core/docs/event-contract-v1.md).  
- Event enum source: `castra-core/src/core/events.rs`; reporter implementation: `castra-core/src/core/reporter.rs`.  
- Severity values align with the Attention Model (info, progress, warn, error, blocker) and must not drift without a coordinated version bump.  
- Harness exposes session metadata (host, port, identity hints, wrapper paths) alongside thread updates. The UI consumes this metadata to populate per-VM controls and `vm_commands.sh` helpers.

## Wiring The UI
- `ChatApp` owns the subscription to the harness stream (named channel or async task). Use `gpui::App::spawn` so IO stays off the main thread.  
- Deserialize JSON payloads into the contract layer (`castra_ui::controller::event`) so components remain decoupled from serde internals.  
- Update `AppState` with dedicated structs: roster agent records, VM status, message log entries, global operation summaries, and cached SSH metadata.  
- Merge bootstrap plan/run output (`BootstrapPlanned`, `BootstrapCompleted`) with harness metadata to maintain the per-VM SSH roster and wrapper hints.  
- Harness events (`HarnessEvent::ThreadStarted`, `HarnessEvent::AgentMessage`, `HarnessEvent::Usage`, etc.) continue to drive Codex transcript updates and usage counters.
## Session Metadata Essentials
- **Bootstrap plan/run:** `BootstrapPlanned` and `BootstrapCompleted` include `ssh` details (user, host, port, identity, options) for each VM. Record these in `AppState` so the UI can render accurate wrapper commands.  
- **Run directories:** `vm_commands.sh` creates run folders under `/run/castra-agent/<run_id>`; expose the `view-output` command in the UI when metadata indicates captured logs.  
- **Usage:** `HarnessEvent::Usage` contains prompt, cached, and completion token counts. Aggregate alongside bootstrap metadata so the footer and diagnostics panes report accurate totals.  
- **Failures:** Relay `HarnessEvent::Failure` messages and bootstrap `BootstrapFailed` diagnostics directly to operators with remediation hints (e.g., rerun bootstrap, inspect guest serial logs).

## Handling Attention Levels
- Map `Severity::Info` and `Severity::Progress` to calm gray/green shades; `Warn` to amber; `Error` to red; `Blocker` to pulsing crimson per Thread 21.  
- Aggregate the highest severity visible to operators and surface it through the status footer and optional window chrome effects.  
- When severity downgrades (e.g. warning resolved), append a resolution message to the log instead of silently clearing cues.

## Command Round-Trips
- Slash commands capture user intent; send structured requests to castra-core (e.g. `UpCommand { config, bootstrap }`).  
- Upon receiving `command.accepted`, persist run metadata (config path, start time).  
- Propagate `command.rejected` details directly into the message log with attention level `warn` or `error` depending on the rejection type.

## Testing With The Minimal Bootstrap Example
- Launch castra-core’s JSON event stream against `examples/minimal-bootstrap`.  
- Pipe the stream into castra-ui via stdin or a lightweight IPC while the UI runs in dev mode.  
- Verify expected sequences: plan → launch → bootstrap steps → summary → cleanup, followed by Codex thread and usage events from the harness.  
- Ensure every event updates exactly one UI facet (card, log, footer) and that no event is silently ignored.

## Further Reading
- [Event Contract v1](../../../castra-core/docs/event-contract-v1.md)  
- [Attention Model draft](../reference/attention_model.md)  
- [UI Vertical Slice: Up](../../../UP.md)
