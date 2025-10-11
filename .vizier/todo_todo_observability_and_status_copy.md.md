Update (SNAPSHOT v0.4)

Evidence
- `status` implemented: prints table with VM, STATE, CPU/MEM, UPTIME, BROKER, FORWARDS; colors when stdout is a TTY; plain text otherwise.
- `logs` implemented: `--tail` shows recent lines per source, `--follow` streams; clear prefixes for broker, QEMU, and serial; gracefully notes when a log file isn't created yet.

Refinement
- BROKER `reachable` state pending guest handshake; currently `waiting|offline`.
- Consider aligning width calculations and truncation for extremely long names.

Acceptance criteria (amended v0.4)
- Status and logs behavior per copy are live. [DONE]
- Enhance broker reachability signal once handshake exists. [NEXT]


---

