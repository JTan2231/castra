# Developer Notes: Consuming Event Contract v1

**Assumed versions:** Castra snapshot v0.10.0-pre · Castra UI 0.1.0 · Event Contract v1 · Attention Model draft v0.1  
**Updated:** 2024-06-02

## What Changed Since Last Version
- Documented the harness-originated `vizier.remote.*` stream and how the UI translates those frames into operator-facing state.

## Contract Recap
- Canonical definitions live in [`castra-core/docs/event-contract-v1.md`](../../../castra-core/docs/event-contract-v1.md).  
- Event enum source: `castra-core/src/core/events.rs`; reporter implementation: `castra-core/src/core/reporter.rs`.  
- Severity values align with the Attention Model (info, progress, warn, error, blocker) and must not drift without a coordinated version bump.  
- Harness streams Vizier telemetry as `vizier.remote.*` events. The UI receives them via `HarnessEvent::VizierRemote` and updates VM panels, usage counters, and toast notifications from those payloads.

## Wiring The UI
- `ChatApp` owns the subscription to the harness stream (named channel or async task). Use `gpui::App::spawn` so IO stays off the main thread.  
- Deserialize JSON payloads into the contract layer (`castra_ui::controller::event`) so components remain decoupled from serde internals.  
- Update `AppState` with dedicated structs: roster agent records, VM status, message log entries, global operation summaries, and Vizier tunnel state.  
- Invoke `AppState::apply_vizier_remote_frame` for every `HarnessEvent::VizierRemote { event }` to ensure reconnect attempts, usage reports, and system logs surface immediately.

## Vizier Remote Stream Essentials
- **Handshake:** `vizier.remote.handshake` carries `protocol_version`, `vm_vizier_version`, optional `log_path`, and capability hints (`echo_latency_hint_ms`, `supports_reconnect`, `supports_system_events`). Record the protocol, show the Vizier build, and surface `state/vizier/<vm>/service.log` so operators can pivot to guest logs quickly.
- **Output/System:** `vizier.remote.output` represents stdout/stderr streaming from the guest; `vizier.remote.system` surfaces service log lines. Route stdout to the transcript, stderr/system frames to the system message rail with attention cues.
- **Health:** `vizier.remote.status`, `...reconnect_attempt`, `...reconnect_succeeded`, and `...disconnected` drive VM health badges. Toast reconnect attempts and mark the VM as `RECONNECTING` until a follow-up `handshake` or `reconnect_succeeded` arrives.
- **Failures:** `vizier.remote.handshake_failed` and `vizier.remote.error` require high visibility (system message + toast). Include remediation hints supplied by the harness to guide operators toward restarting Vizier or updating protocol versions.
- **Usage:** `vizier.remote.usage` contains prompt, cached, and completion token counts. Aggregate alongside Codex usage so the footer and diagnostics panes report accurate totals.
- **Acknowledgements:** `vizier.remote.ack` echoes control frame IDs back to the UI. Log them in the transcript when troubleshooting command delivery.

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
- Verify expected sequences: plan → launch → bootstrap steps → summary → cleanup, followed by Vizier handshake + status frames.  
- Ensure every event updates exactly one UI facet (card, log, footer) and that no event is silently ignored.

## Further Reading
- [Event Contract v1](../../../castra-core/docs/event-contract-v1.md)  
- [Attention Model draft](../reference/attention_model.md)  
- [UI Vertical Slice: Up](../../../UP.md)

