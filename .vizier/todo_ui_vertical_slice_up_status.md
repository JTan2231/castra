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

Threads: Thread 20 and 21; consumes 12, 13.Progress (evidence):
- UI consumes HarnessEvent stream for Codex/Vizier and renders into message_log; start/finish statuses and command outputs appear with distinct kinds (Vizier Command vs Vizier System).
- status_footer now displays token usage summaries (Codex and Vizier) and retains operation status; layout widened and responsive.
- Prompt shell supports a Stop control while a Codex turn is active.
- Message log ships with collapse for repetitive tool/reasoning outputs and truncation notice to keep UI responsive.

Gaps to acceptance:
- Roster remains VM-centric; pivot to agent roster pending (Thread 31).
- Live per-agent progress badges and aggregate counts not yet implemented; timers exist but lack Up-specific progress semantics.
- Completion banner for Up summary not yet wired from core/harness events.
- Click-through to durable logs: vm_commands.sh now supports `--wait` and `view-output`, but UI affordance is not yet linked.

Next steps:
- Wire harness Up lifecycle events into roster badges and aggregate counts; update status_footer to show Up progress and durations.
- Add link/command to open view-output for the selected run when available.

Threads: 20, 21; consumes 12, 13. Anchors unchanged.

---

