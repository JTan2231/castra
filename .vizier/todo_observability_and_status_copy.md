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

---
Update (SNAPSHOT v0.3)

Evidence
- `status` and `logs` remain NYI, but CLI help and NYI hints now point to repo-root TODOs.
- `ports` exists and will inform status (planned vs active forwards) once VMs run.

Copy draft (source of truth for implementation)
- `castra status` (TTY colors implied; plain text when not a TTY):

```
VM           STATE         CPU/MEM   UPTIME    BROKER    FORWARDS
web-1        running       2/2048M   00:12:34  reachable  0.0.0.0:8080->80/tcp, 2222->22/tcp
worker-a     starting      1/1024M   —         waiting    3000->3000/tcp
db           stopped       2/4096M   —         —          —

Legend: BROKER reachable = host broker handshake OK; waiting = broker up, guest not connected.
States: stopped | starting | running | shutting_down | error
Exit codes: 0 on success; non-zero if any VM in error.
```

- `castra logs --tail 200 --follow`
```
[host-broker] 12:34:56 INFO listening on 127.0.0.1:9000
[vm:web-1:qemu] 12:34:57 INFO qemu-system-x86_64 launched pid=12345
[vm:web-1:serial] 12:34:58 login[233]: root login on 'ttyS0'
[vm:worker-a:qemu] 12:35:02 WARN retrying tap attach (attempt 2)
```
Rules:
- Prefixes: host-broker | vm:<name>:qemu | vm:<name>:serial
- `--follow` streams until interrupted; without it, show tail then exit.
- Respect `--tail N` for initial history. Auto-disable colors when not a TTY.

Refinement
- Integrate live port/proc info so `status` FORWARDS shows only active mappings when VM is running; otherwise planned.
- Ensure `logs` gracefully degrades when a source is unavailable (e.g., no serial log yet) with a one-line notice per source.

Acceptance criteria (amended v0.3)
- Implement status table with above columns and semantics.
- Implement logs prefixes and follow/tail behavior per copy.
- Color/TTY detection implemented; plain text when redirected.
---

---

