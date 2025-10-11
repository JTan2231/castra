Thread: Observability and status legibility (depends on SNAPSHOT v0.1)

Goal
- Provide consistent, human-friendly output for status and logs.

Acceptance criteria
- `castra status` shows per-VM table with name, state, CPU/mem, uptime, broker reachability.
- `castra logs` tails recent host-broker logs and QEMU stderr/stdout with clear prefixes.
- Color output when TTY; plain text when redirected.

Notes
- Avoid prescribing logger libraries; focus on observable output behavior.
---
Update (SNAPSHOT v0.2)

Evidence
- `status` and `logs` commands exist but return NYI errors with tracking hints pointing to `.vizier/*` (mismatch with repo layout).

Refinement
- Align NYI tracking hints to existing TODO filenames.
- Draft the concrete status table columns and example outputs so copy can be implemented directly once data is available.

Acceptance criteria (amended)
- NYI messages guide users to the correct local TODO.
- A sample `castra status` output (in TODO) is used as the copy source during implementation.

---

