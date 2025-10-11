---
Update (SNAPSHOT v0.3)

Evidence
- `status` and `logs` remain NYI; `ports` provides a precedent for table output and warning surfacing.

Refinement
- Draft concrete `status` examples informed by current data we can obtain in MVP (pidfile/process state, config-declared CPU/mem, planned forwards). Broker reachability can be TBD/unknown in MVP unless broker exists.

Copy source (sample)
- Example: `castra status` (no VMs running)
  VM         STATE     CPU  MEM      UPTIME   FORWARDS
  devbox     stopped   2    2048MiB  —        2222->22/tcp, 8080->80/tcp

- Example: `castra status` (VM running, uptime derived from pidfile mtime)
  VM         STATE     CPU  MEM      UPTIME   FORWARDS
  devbox     running   2    2048MiB  00:03:42 2222->22/tcp, 8080->80/tcp

Acceptance criteria (amended)
- Implementers can lift this table and placeholders directly; unknown values display as `—`.
- NYI messages continue pointing to this TODO; once implemented, update to reflect live data sources.


---

