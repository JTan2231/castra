
---
Update (SNAPSHOT v0.5)

Evidence
- Preflight before up: CPU and memory headroom checks (warn/fail thresholds with friendly messages and --force override), free-disk checks across state/logs/overlay directories (warn at ~2GiB, fail at ~500MiB), host port conflicts (including broker overlap), qemu-system presence check. Warnings surfaced immediately; failures block unless --force.

Refinement
- Consider signal handling for castra itself to allow graceful cancellation during long operations.

Acceptance criteria (amended v0.5)
- Documented thresholds are enforced and communicated; --force prints explicit override warnings. [DONE]
- Graceful interrupt handling during up. [OPEN]


---

---
Update (SNAPSHOT v0.7)

Evidence
- Preflight integrated into `up`: CPU/mem headroom with thresholds and --force; disk free checks across state/logs/overlay dirs; port conflict checks including broker; qemu-system presence enforced; friendly, actionable copy.

Open
- Graceful interrupt handling during long operations (downloads, conversions) to avoid partial side effects.


---

