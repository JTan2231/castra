
---
Update (SNAPSHOT v0.5)

Evidence
- status prints table with VM | STATE | CPU/MEM | UPTIME | BROKER | FORWARDS; colors on TTY only. BROKER shows waiting when listener up, offline otherwise. Legend printed.
- logs tails broker and per-VM qemu/serial with labeled prefixes; respects --tail/--follow; degrades gracefully when files absent.

Refinement
- Add broker "reachable" state once guest handshake lands.
- Consider truncation/ellipsis for very long VM names to preserve table shape.

Acceptance criteria (amended v0.5)
- Status and logs behavior match copy above (color/TTY detection, prefixes, graceful gaps). [DONE]
- BROKER: add reachable once handshake implemented. [OPEN]


---

---
Update (SNAPSHOT v0.7)

Evidence
- Status table and legend implemented with color-on-tty; widths computed for stable layout. Logs tailer supports --tail/--follow, labels sources with colored prefixes; degrades gracefully when files are missing.

Open
- Add reachable state once broker handshake exists; consider truncation/ellipsis for very long VM names.


---

