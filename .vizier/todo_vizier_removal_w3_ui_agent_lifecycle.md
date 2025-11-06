Thread 50 — Vizier Removal — Workstream 3: UI agent lifecycle rewrite

Tension
- UI currently depends on vizier.remote events for roster/status/diagnostics; post-pivot it must own SSH sessions to agent runtimes.

Desired behavior (product level)
- UI discovers agent endpoints (via config or harness metadata), establishes SSH sessions per agent/VM, manages connection lifecycle, and renders status/diagnostics accordingly.
- Transcript, prompt shell, and message log operate over direct agent sessions.

Acceptance criteria
- UI starts with a visible agent roster using new session manager; first-message roundtrip works; reconnect behavior is user-visible and non-disruptive.
- All Vizier strings and code paths removed from UI; components updated (roster_sidebar, vm_fleet, status_footer, message_log, prompt_shell).

Pointers
- castra-ui/src/state/* (remove vizier fields; add agent session manager)
- castra-ui/src/components/* (rebind to session manager)
- castra-ui/src/controller/*, app/actions.rs (wire commands)

Notes
- Keep transport/protocol choices open; defer generalized agent transport layer; focus on SSH session management and UX outcomes.Status update (v0.12.2)
- Progress: Vizier strings/paths and event handling removed from UI state/components; user input now routes to active agent and Codex; catalog launch flow guards during up.
- Remaining scope (acceptance-critical):
  1) Introduce a session manager for direct SSH agent sessions (discover endpoints from BootstrapPlanned/Completed ssh metadata; maintain per-VM connection state; expose send/interrupt helpers mapped to vm_commands.sh). Acceptance: first-message roundtrip visible in prompt_shell; reconnect UI states.
  2) Bind roster badges and status_footer to session presence/health; remove residual vm_fleet dependencies where roster suffices.
  3) Transcript integration for remote stdout/stderr via vm_commands.sh view-output when --wait is used; collapse noise per attention model.
  4) Tests: unit coverage for routing to active agent; integration smoke using minimal-bootstrap to assert plan→steps→ready→first-message.
- Dependencies: event-contract v1 ssh fields; vm_commands.sh interface; Harness metadata surfaces.
- Risk notes: avoid hardcoding paths; prefer contract surfaces and environment-driven wrappers.

---

