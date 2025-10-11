Thread: Observability and status legibility (depends on SNAPSHOT v0.1)

Goal
- Provide consistent, human-friendly output for status and logs.

Acceptance criteria
- `castra status` shows per-VM table with name, state, CPU/mem, uptime, broker reachability.
- `castra logs` tails recent host-broker logs and QEMU stderr/stdout with clear prefixes.
- Color output when TTY; plain text when redirected.

Notes
- Avoid prescribing logger libraries; focus on observable output behavior.
