# Tutorial: Run Your First `/up`

**Assumed versions:** Castra snapshot v0.9.9 · Castra UI 0.1.0 · Event Contract v1 · Attention Model draft v0.1  
**Updated:** 2024-05-21

## What changed since last version
- First publication: end-to-end walkthrough using the Minimal Bootstrap example.

## Before you begin
- Install workspace dependencies and ensure virtualization is available.  
- Fetch the demo image shipped with the repo or rebuild the Alpine profile referenced by `examples/minimal-bootstrap`.  
- Confirm `cargo` can build both `castra-core` and `castra-ui` targets.  
- Skim the [Minimal Bootstrap README](../../../castra-core/examples/minimal-bootstrap/README.md) to understand the assets the run will use.

## Start the UI
1. From the workspace root, launch the UI:  
   ```bash
   cargo run -p castra-ui
   ```  
2. A window opens centered on screen with the prompt focused automatically (thanks to `ChatApp::focus_prompt`).  
3. Toggle the roster with `Ctrl+B`/`Cmd+B` if you want the full layout; ensure the `ASSIST` agent is active in the footer.

## Launch `/up`
1. In the prompt, enter:  
   ```
   /up --config castra-core/examples/minimal-bootstrap/castra.toml --bootstrap=always
   ```  
   The vertical slice (Thread 20) wires this command to castra-core, emitting `command.requested` and `command.accepted` events.  
2. The roster posts a system message confirming that `/up` started; the VM fleet cards begin updating as events arrive.  
3. Message log scrolls through planning details (readiness wait, SSH target, environment keys) derived from `BootstrapPlanned`.

## Follow the run
- **VM Fleet:** Cards note `OverlayPrepared`, `VmLaunched`, and each bootstrap step. Offline → Online flips occur with `VmLaunched` and `ShutdownComplete`.  
- **Message Log:** Progress messages honor severity from the attention model. Warnings (e.g. verification retries) appear in amber; blockers pulse red.  
- **Status Footer:** Shows aggregate progress (e.g. “Bootstrap verify (1/4)”) and escalates the attention cue if any VM emits an error.  
- **Roster:** Active agent remains `ASSIST`; if the run spawns additional automation, expect `agent.status` updates to inform you which persona is speaking.

## Understand ephemerality
- The Minimal Bootstrap example uses overlays, so expect `EphemeralLayerDiscarded` after shutdown.  
- The message log surfaces reclaimed bytes; the VM card note clears to indicate the workspace is clean.  
- Use this moment to highlight the difference between ephemeral (`overlay: true`) and persistent runs (future docs will cross-link here).

## After completion
- Success surfaces as `operation.summary` with total duration and next steps (e.g. `/down` or `/status`).  
- Failures include remediation hints sourced from the Event Contract—typically environment adjustments or verification guidance.  
- Re-run `/up` to validate idempotency; history navigation (`↑`) makes this fast.

## Further reading
- [Event Contract v1](../../../castra-core/docs/event-contract-v1.md)  
- [Attention Model draft](../reference/attention_model.md)  
- [UI Vertical Slice: Up](../../../UP.md)
