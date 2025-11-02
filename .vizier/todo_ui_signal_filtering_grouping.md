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

