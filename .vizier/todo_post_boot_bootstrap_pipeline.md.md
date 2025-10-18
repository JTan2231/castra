
---
Delta (align with current snapshot)
- JSON stream gains a terminal BootstrapSummary event carrying resolved policy (auto|always|skip), effective inputs, per-step durations, outcome, and durable log path. In-flight event schema remains unchanged.
- `--plan` emits the same BootstrapSummary shape with outcome=Planned and zero durations; exits non-zero on invalid config.

Acceptance additions
- In JSON mode, a terminal BootstrapSummary is always present for each VM; field names/types are stable and documented.
- `--plan --json` outputs deterministic BootstrapSummary entries in input order with outcome=Planned; no side effects.
---

---

