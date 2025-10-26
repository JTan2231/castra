# Attention Model Draft (Thread 21)

**Assumed versions:** Castra snapshot v0.9.9 · Castra UI 0.1.0 · Event Contract v1 · Attention Model draft v0.1  
**Updated:** 2024-05-21

## What changed since last version
- Placeholder published so onboarding docs can cross-link; full design lands with Thread 21 deliverable.

## Severity bands
- **Info:** Background state changes or confirmations; rendered in neutral gray.  
- **Progress:** Active work in flight; lean green tones, optional spinner accents.  
- **Warn:** Recoverable issues needing attention; amber highlights and subtle pulse.  
- **Error:** Blocked work; bold red text and roster/footer escalation.  
- **Blocker:** Requires immediate action; pulsing crimson, optional modal nudge.

## Grouping and rate limiting
- Collapse repeated progress updates, keeping the latest timestamp visible.  
- Summarize bursts (e.g. bootstrap step logs) into a single expandable entry in the message log.  
- Resolution events should close or dim prior alerts instead of deleting them.

## Remediation hints
- Pair warn/error/blocker severities with actionable guidance supplied by the Event Contract (`remediation` strings).  
- Display hints inline in the message log and optionally mirror them on the affected VM card.

## Next steps
- Thread 21 will publish the full design spec plus visual tokens.  
- Update this document once the spec stabilizes; include changelog entries tied to contract version bumps.

## Further reading
- [Event Contract v1](../../../castra-core/docs/event-contract-v1.md)  
- [UI Vertical Slice: Up](../../../UP.md)
