Title: Agent-first interaction model — remove runtime VM choice

Context
- We are simplifying the system by prioritizing multiple agents over multiple VMs. Runtime VM selection (per-VM targeting/choosing) adds complexity and competes with the attention model.
- Thread 31 and Thread 21 drive this pivot; coordinated with Thread 20 (harness stream) and Thread 30 (busless cutover).

Intent
- Present agents (and optional agent groups/roles) as the primary addressing unit for operations and events. Eliminate UX/flags that require picking specific VMs at runtime.

Product scope and acceptance
- UI
  - The roster displays agents, not VMs. No UI affordance exists to pick individual VMs for command routing.
  - Any targeting widget addresses agent identity or group/role. Empty selection defaults to the active attention scope.
  - vm_fleet and related docs/examples are removed or redirected to an agent roster equivalent.

- CLI/Core
  - Flags/options that select VMs are removed or emit deprecation guidance without functional effect. Defaults operate on the agent-defined scope.
  - Core operations accept an agent scope (implicitly via harness/session) rather than per-VM selectors.

- Event Contract
  - Events include agent.id and optional agent.role/group. No per-VM selection semantics are required to interpret streams.

Anchors
- castra-ui/src/components/{vm_fleet.rs, roster_sidebar.rs}
- castra-ui/docs/components/vm_fleet.md
- castra-core/src/core/options.rs; castra-core/src/app/common.rs
- castra-harness/src/events.rs; castra-harness/src/session.rs

Thread links
- Serves Thread 31 (Agent-first) and updates Thread 21 (Attention model). Coordinates with Thread 20 (unified stream) and Thread 30 (removing broker paraphernalia).
