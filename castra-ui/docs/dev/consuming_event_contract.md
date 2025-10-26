# Developer Notes: Consuming Event Contract v1

**Assumed versions:** Castra snapshot v0.9.9 · Castra UI 0.1.0 · Event Contract v1 · Attention Model draft v0.1  
**Updated:** 2024-05-21

## What changed since last version
- New page outlining how castra-ui reads and reacts to Event Contract v1.

## Contract recap
- Canonical definitions live in [`castra-core/docs/event-contract-v1.md`](../../../castra-core/docs/event-contract-v1.md).  
- Event enum source: `castra-core/src/core/events.rs`; reporter implementation: `castra-core/src/core/reporter.rs`.  
- Severity values align with the Attention Model (info, progress, warn, error, blocker) and must not drift without a coordinated version bump.

## Wiring the UI
- `ChatApp` will own a subscription to the event stream (named channel or async task). Use `gpui::App::spawn` to keep IO off the main thread.  
- Deserialize JSON payloads into a thin contract layer (`castra_ui::controller::event`) so component code stays agnostic of serde internals.  
- Update `AppState` with dedicated structs: roster agent records, VM status, message log entries, and global operation summaries.  
- Reuse existing component render functions (`roster_sidebar::agent_row`, `vm_fleet::vm_card`, `message_log::message_log`) so UI diffing remains scoped to state changes.

## Handling attention levels
- Map `Severity::Info` and `Severity::Progress` to calm gray/green shades; `Warn` to amber; `Error` to red; `Blocker` to pulsing crimson per Thread 21.  
- Aggregate the highest severity visible to operators and surface it through the status footer and optional window chrome effects.  
- When severity downgrades (e.g. warning resolved), append a resolution message to the log instead of silently clearing cues.

## Command round-trips
- Slash commands capture user intent; send structured requests to castra-core (e.g. `UpCommand { config, bootstrap }`).  
- Upon receiving `command.accepted`, persist run metadata (config path, start time).  
- Propagate `command.rejected` details directly into the message log with attention level `warn` or `error` depending on the rejection type.

## Testing with the minimal bootstrap example
- Launch castra-core’s JSON event stream against `examples/minimal-bootstrap`.  
- Pipe the stream into castra-ui via stdin or a lightweight IPC while the UI runs in dev mode.  
- Verify expected sequences: plan → launch → bootstrap steps → summary → cleanup.  
- Ensure every event updates exactly one UI facet (card, log, footer) and that no event is silently ignored.

## Further reading
- [Event Contract v1](../../../castra-core/docs/event-contract-v1.md)  
- [Attention Model draft](../reference/attention_model.md)  
- [UI Vertical Slice: Up](../../../UP.md)
