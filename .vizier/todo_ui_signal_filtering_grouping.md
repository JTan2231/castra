# UI signal filtering and grouping

Goal: Reduce noise from high-volume agent events without losing critical information.

Acceptance criteria:
- Message log groups identical or similar events (burst coalescing), shows counts and most recent timestamp.
- Per-VM context panes show last N significant events with quick expand for full history.
- Throttling ensures UI stays responsive under bursty streams; no dropped critical events.

Scope:
- Product-level behavior; keep implementation open to multiple strategies (coalescing by key, debounce windows, etc.).

Anchors: castra-ui/src/components/message_log.rs; castra-ui/src/state/; castra-ui/src/controller/.

Threads: Thread 21.