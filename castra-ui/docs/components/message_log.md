# Message Log

**Assumed versions:** Castra snapshot v0.9.9 · Castra UI 0.1.0 · Event Contract v1 · Attention Model draft v0.1  
**Updated:** 2024-05-21

## What changed since last version
- First revision, documenting severity styling and grouping expectations.

## Role in the layout
- Central column showing chronological activity: timestamps, speaker labels, and message text.  
- Powers situational awareness for operators by merging system notices, user commands, agent replies, and lifecycle events.  
- Scrolls independently so long-running workflows remain reviewable without hiding the prompt.

## Event Contract mapping
- Render every `message` event verbatim with timestamp and severity.  
- Promote `command.accepted` / `command.rejected` events to system messages so users immediately see success or remediation hints.  
- Bootstrap and VM lifecycle events funnel through localized copy (e.g. “`vm-alpha` readiness confirmed (SSH)”) before landing in the log, pairing with the VM card highlight.  
- Implement rate-limited grouping (Thread 21) to collapse repetitive progress updates while keeping the latest detail visible.

## Interaction patterns
- Keep the log focus-friendly; clicking does not steal prompt focus unless selecting text.  
- Long messages wrap; preserve monospace metrics to match the rest of the shell aesthetic.  
- Empty logs render a placeholder “Awaiting input…” message so onboarding feels deliberate instead of broken.

## Minimal bootstrap walkthrough
- Launch `/up` using [examples/minimal-bootstrap](../../../castra-core/examples/minimal-bootstrap/README.md).  
- Watch the log emit: `/up --config …` (user), “Planning bootstrap for devbox-0” (system), followed by step-by-step bootstrap progress messages at `info` severity.  
- Any verification warning (for example simulated retries) should emit at `warn` severity, instructing the operator to inspect the VM card or rerun with overrides.  
- On success, `operation.summary` posts duration and overlay cleanup; if the run fails, the `error` severity message includes the remediation hint supplied by the contract.

## Further reading
- [Event Contract v1](../../../castra-core/docs/event-contract-v1.md)  
- [Attention Model draft](../reference/attention_model.md)  
- [UI Vertical Slice: Up](../../../UP.md)
