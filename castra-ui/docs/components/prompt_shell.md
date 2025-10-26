# Prompt Shell

**Assumed versions:** Castra snapshot v0.9.9 · Castra UI 0.1.0 · Event Contract v1 · Attention Model draft v0.1  
**Updated:** 2024-05-21

## What changed since last version
- Initial documentation of prompt behavior, history, and command flow.

## Role in the layout
- Anchors the bottom of the UI as the primary input surface.  
- Accepts plain text (routed to the active agent) or slash commands (parsed by `controller::command`).  
- Shares the same dark palette as the roster to emphasize continuity with the terminal motif.

## Event Contract mapping
- Plain text submissions become `user.input` events tagged with the active agent.  
- Slash commands emit `command.requested` followed by `command.accepted`/`command.rejected`; the controller forwards structured payloads to castra-core.  
- Prompt feedback (echoed messages, agent responses) surfaces as `message` events consumed by the message log.  
- History navigation is local UI state, but accepted commands still append to the event stream for auditability.

## Interaction patterns
- `Enter` submits; `Shift+Enter` is reserved for multiline expansion in future revisions.  
- `↑`/`↓` rotate through history, preserving the current draft with `PromptInput::store_current_as_draft`.  
- Mouse clicks set the cursor position without dropping focus; the component tracks layout bounds to convert coordinates into grapheme offsets.  
- `/help`, `/agents`, and `/switch <id>` ship today; `/up` binds into the Thread 20 vertical slice.

## Minimal bootstrap walkthrough
- Run `/up` in the prompt to launch the [Minimal Bootstrap example](../../../castra-core/examples/minimal-bootstrap/README.md).  
- The command parses as `command::Command::Switch` or `Command::Unknown` today; once Thread 20 lands, `/up` will dispatch to castra-core and stream events back.  
- Observe system echoes in the message log confirming command acceptance and route adjustments.

## Further reading
- [Event Contract v1](../../../castra-core/docs/event-contract-v1.md)  
- [Attention Model draft](../reference/attention_model.md)  
- [UI Vertical Slice: Up](../../../UP.md)
