# UI vertical slice: Up + live status

Goal: castra-ui can launch an Up operation and render live per-VM progress with clear attention cues.

Acceptance criteria:
- Controller initiates Up using the contract boundary; event stream drives state.
- Components updated:
  - roster_sidebar: per-VM badge with state (pending, running, success, failed, blocked) and subtle progress.
  - status_footer: aggregate counts + active timers; ephemerality reminder when applicable.
  - message_log: grouped events; collapse repetitive logs; click-through to durable logs when present.
- Completion banner derived from summary events (e.g., BootstrapSummary or Up summary) with duration and next actions.

Scope:
- Focus on Up; down/status/clean can be stubs that render basic signals.
- Maintain responsiveness; UI must not freeze on bursty event streams.

Anchors: castra-ui/src/controller/{mod.rs,command.rs}; castra-ui/src/components/{roster_sidebar.rs,status_footer.rs,message_log.rs,vm_fleet.rs}.

Threads: Thread 20 and 21; consumes 12, 13.