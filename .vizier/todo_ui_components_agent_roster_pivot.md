Title: Pivot UI components from VM fleet to Agent roster

Context
- VM-centric components (vm_fleet, roster_sidebar) and docs teach per-VM selection. We are moving to an agent-first model (Thread 31).

Intent
- Replace/remove VM-centric components and their docs with agent-centric equivalents that reflect the attention model.

Product scope and acceptance
- Replace vm_fleet with agent_roster (name flexible) that lists agents with status/role and selection for attention only (not routing per-VM).
- roster_sidebar mirrors agent roster and attention state; no per-VM command routing affordance.
- Update docs: castra-ui/docs/components/{vm_fleet.md, roster_sidebar.md} to agent-first terminology and examples.
- Update tutorials to show agent-centric Up flow; remove VM choice walkthroughs.

Anchors
- castra-ui/src/components/{vm_fleet.rs, roster_sidebar.rs}
- castra-ui/docs/components/{vm_fleet.md, roster_sidebar.md}
- castra-ui/docs/tutorials/first_up.md

Thread links
- Advances Thread 31 and Thread 21; aligns with Thread 20 (UI subscribes to harness stream).