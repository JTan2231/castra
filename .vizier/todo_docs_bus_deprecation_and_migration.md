Thread: 30 + 20 — Docs alignment for bus removal and vizier-first

Goal: Update all docs and examples to remove bus/broker references and introduce the harness vizier SSH model.

Acceptance criteria:
- castra-core/docs/event-contract-v1.md updated: remove broker/bus events; add vizier.ssh family, version preamble; include samples.
- castra-ui/docs/dev/consuming_event_contract.md updated to subscribe to the harness stream instead of core/broker.
- README.md and AGENTS.md reflect harness-first operation and do not mention the bus.
- scripts/ docs removed; examples updated to run via harness.

Anchors:
- castra-core/docs/*; castra-ui/docs/*; top-level README.md; AGENTS.md; HARNESS.md.

Migration note:
- Add a short "Breaking change" section with replacement guidance for any previous bus-centric workflows.Snapshot v0.10.0-pre update
- Add deprecation section covering broker/bus-era features and the busless pivot rationale.
- Migration guidance:
  - Mapping from bus commands to vm_commands.sh remote runner (send/interrupt/list/view-output, --wait).
  - What disappears now vs later; timeline and version guardrails.
- Cross-link: Thread 30 acceptance; vm_commands.sh usage examples.

---

