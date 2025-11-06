# UI signal filtering and grouping

Goal: Reduce noise from high-volume agent events without losing critical information.

Acceptance criteria:
- Message log groups identical or similar events (burst coalescing), shows counts and most recent timestamp.
- Per-VM context panes show last N significant events with quick expand for full history.
- Throttling ensures UI stays responsive under bursty streams; no dropped critical events.

Scope:
- Product-level behavior; keep implementation open to multiple strategies (coalescing by key, debounce windows, etc.).

Anchors: castra-ui/src/components/message_log.rs; castra-ui/src/state/; castra-ui/src/controller/.

Threads: Thread 21.Progress (evidence):
- Collapsible messages shipped in castra-ui (message_log.rs, state/mod.rs):
  - Reasoning and Tool outputs are collapsed by default with a preview line and a “hidden — click to expand” affordance.
  - Click-to-toggle implemented via ChatState::toggle_message_at; UI wires handler through shell → message_log.
  - Added soft log limit with truncation notice (dropped older messages count) to preserve responsiveness.
  - Scroll-stick-to-bottom logic improved with wheel listener + bottom tolerance.

Impact:
- Reduces visual noise and improves responsiveness under bursty output.

Remaining work to satisfy acceptance:
- Grouping/coalescing of identical or similar events over a debounce window with counts and latest timestamp.
- Per-agent (formerly per-VM) context panes: last N significant events with quick expand for full history.
- Ensure throttling preserves priority of critical events (e.g., failures) and never collapses them away.

Anchors updated:
- castra-ui/src/components/{message_log.rs,shell.rs}
- castra-ui/src/state/mod.rs

Threads: 21 — carry forward; aligns with agent-first pivot from Thread 31.

---

Snapshot v0.10.0-pre update
- Current: collapsible messages, scroll protections, truncation soft limit.
- Next acceptance:
  - Group sequential tool/reasoning bursts into a collapsible group with one-click expand/collapse-all.
  - Maintain sticky-to-bottom behavior without jumping when expanding groups.
  - Telemetry: footer continues token tallies; clicking a tally opens the agent context pane (stub acceptable in first slice).
- Anchors: castra-ui/src/components/message_log.rs, castra-ui/src/state/mod.rs, castra-ui/src/components/status_footer.rs.

---

Thread link: Thread 21 — Operational clarity (Attention model). Context: Stream source is vizier.remote.*; translator shim removed. Acceptance: status_footer and vm_fleet show consolidated health/attention states computed from vizier.remote.*; grouping by VM and cause; interactions to expand to raw events; defaults minimize churn while preserving first-fault visibility.

---

Pivot alignment (Vizier removal):
- Replace all VM/vizier.remote references with agent session terminology.
- Scope now explicitly per-agent with optional grouping by agent role.
- Acceptance unchanged, with added constraint: grouping must never collapse first-fault errors originating from an agent session.

Anchors update:
- castra-ui/src/components/{message_log.rs,shell.rs,status_footer.rs}
- castra-ui/src/state/* (agent-centric state)

Thread link update:
- Thread 21 — Operational clarity; input signals come from agent sessions + harness metadata (Snapshot v0.12).

---

