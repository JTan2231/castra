# Status Footer

**Assumed versions:** Castra snapshot v0.9.9 · Castra UI 0.1.0 · Event Contract v1 · Attention Model draft v0.1  
**Updated:** 2024-05-21

## What changed since last version
- Initial draft covering shortcut hints and aggregate attention hooks.

## Role in the layout
- Fixed bar between the message log and prompt, mirroring terminal status lines.  
- Displays the active agent label, current time, and contextual shortcut reminders (focus prompt, agent slots, toggle roster, history, `/help`).  
- Future revisions integrate aggregate operation health so operators can glance for blockers without scrolling.

## Event Contract mapping
- `operation.progress` supplies counts of active tasks; display them inline once exposed.  
- Map highest outstanding attention level across all VMs (info → warn → error → blocker) to color the label or inject subtle animations.  
- `command.accepted` for `/switch` or `/up` triggers transient hints (e.g. “UP started – follow progress in the log”) without overwhelming the message pane.

## Interaction patterns
- Treat the footer as purely informational; no buttons to click, keeping keyboard focus on the prompt.  
- When the attention level escalates to `error` or `blocker`, pulse the footer text for three seconds to draw the eye, then revert to steady state.  
- Honor platform-specific shortcuts: macOS uses `Cmd`, everything else falls back to `Ctrl`.

## Minimal bootstrap walkthrough
- During the [Minimal Bootstrap](../../../castra-core/examples/minimal-bootstrap/README.md) run, the footer notes the active agent (`ASSIST`) handling `/up`.  
- As bootstrap enters `verify`, emit `operation.progress` updates reflecting remaining steps; on success the hint line can briefly show “All VMs healthy”.  
- If the run fails, elevate attention to `error` and prompt the operator to open the message log entry that carries remediation details.

## Further reading
- [Event Contract v1](../../../castra-core/docs/event-contract-v1.md)  
- [Attention Model draft](../reference/attention_model.md)  
- [UI Vertical Slice: Up](../../../UP.md)
