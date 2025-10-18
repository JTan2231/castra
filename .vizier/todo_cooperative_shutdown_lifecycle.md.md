
---
Delta (clarify surfacing + docs)
- CLI help and `--json` now explicitly surface the effective cooperative/TERM/KILL timeouts per run; examples include 0ms Unavailable and ChannelError variants with brief remediation hints.

Acceptance additions
- `castra down --json` includes effective timeout fields alongside outcomes; CLI help shows defaults and notes how config overrides resolve.
---

---

