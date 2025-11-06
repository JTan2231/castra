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
- Keep transport/protocol choices open; defer generalized agent transport layer; focus on SSH session management and UX outcomes.