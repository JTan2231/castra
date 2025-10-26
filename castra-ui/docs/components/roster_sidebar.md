# Roster Sidebar Component

**Assumed versions:** Castra snapshot v0.9.9 · Castra UI 0.1.0 · Event Contract v1 · Attention Model draft v0.1  
**Updated:** 2024-05-21

## What changed since last version
- Initial component guide derived from Thread 22 scope.

## Role in the layout
- Lives on the far-left rail when enabled, reserving 200 px for agent rows.  
- Shows the uppercase agent label alongside the latest status phrase (e.g. `ONLINE`, `IDLE`, `STANDBY`).  
- Darker highlight and brighter text mark the active routing target for prompt submissions.  
- A header badge (`AGENTS`) keeps the component scannable in game-style dark modes.

## Event Contract mapping
- Consume `agent.status` events to refresh each row’s status text and severity tint.  
- A future `agent.attention` payload can alter text color using the Attention Model mapping (warn → amber, error → red).  
- `command.accepted` with scope `switch-agent` updates the active row; `command.rejected` should surface remediation in the message log instead of flipping highlight.  
- When the sidebar is hidden, roster state persists in `AppState::roster` so re-opening keeps the last active agent.

## Interaction patterns
- Toggle visibility with `Ctrl+B`/`Cmd+B`; the UI state flips without discarding agent data.  
- Click on a row to switch agents; the handler in `ChatApp::render` wires mouse down events to state updates.  
- Keyboard slots map `Ctrl+1…9` (`Cmd+1…9`) to roster positions, leveraging the ordering in `RosterState::agents`.  
- Switching agents pushes a system message confirming the new route so the message log records who owns subsequent prompt traffic.

## Minimal bootstrap walkthrough
- After launching `/up` with the [Minimal Bootstrap Example](../../../castra-core/examples/minimal-bootstrap/README.md), watch for `agent.status` updates as the automation agent relays bootstrap progress.  
- If the run surfaces a warning (e.g. verification retries), the attention model elevates the agent label color, prompting you to inspect the message log.  
- Cooperative shutdown events emitted at the end of the run reset the status text to `ONLINE` once the VM returns to idle.

## Further reading
- [Event Contract v1](../../../castra-core/docs/event-contract-v1.md)  
- [Attention Model draft](../reference/attention_model.md)  
- [UI Vertical Slice: Up](../../../UP.md)
