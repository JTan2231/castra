# VM Fleet Columns

**Assumed versions:** Castra snapshot v0.9.9 · Castra UI 0.1.0 · Event Contract v1 · Attention Model draft v0.1  
**Updated:** 2024-05-21

## What changed since last version
- Initial publication covering cards, attention cues, and contract bindings.

## Role in the layout
- The fleet frames the message log with left/right columns, each card summarizing one VM.  
- Header (`VM FLEET`) anchors context; cards expose name, online/offline status, project label, and the latest lifecycle note.  
- Color dot (6 px square) conveys online state; future revisions extend this to reflect attention level pulses.

## Event Contract mapping
- Subscribe to `vm.lifecycle` events to flip the online indicator and status label (`ONLINE`, `OFFLINE`, `BOOTING`, etc.).  
- `bootstrap.plan` and `bootstrap.step` events feed the “Last message” line with the most recent human-readable detail.  
- `ephemeral.overlay.discarded` events append clean-up summaries (reason, bytes reclaimed) beneath the project line using the attention model’s info severity.  
- `operation.summary` for `/up` or `/down` should push a completion banner to the associated card, keeping intent and result coupled.

## Interaction patterns
- Cards are static in v0.1.0 but already sized for hover tooltips. Keep the outer container scrollable so large fleets remain accessible.  
- Maintain deterministic ordering (alphabetical) so keyboard focus and logs stay predictable when the underlying vector updates.  
- When no VMs exist, render a placeholder card describing how to run `/up` or import a config; doing so avoids empty columns and guides onboarding.

## Minimal bootstrap walkthrough
- Run `/up` against [examples/minimal-bootstrap](../../../castra-core/examples/minimal-bootstrap/README.md). The first meaningful events are `OverlayPrepared` followed by `VmLaunched`; the corresponding card turns green with `ONLINE`.  
- As bootstrap progresses, expect `BootstrapStep` events (`plan`, `transfer`, `apply`, `verify`) to stream into the card detail. Errors flip the attention color and propagate into the message log.  
- When the run completes, a `BootstrapCompleted` event updates the note with total duration, while any `EphemeralLayerDiscarded` event after shutdown clears residual overlays.

## Further reading
- [Event Contract v1](../../../castra-core/docs/event-contract-v1.md)  
- [Attention Model draft](../reference/attention_model.md)  
- [UI Vertical Slice: Up](../../../UP.md)
