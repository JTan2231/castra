
---
Refinement (TTY UX + durability)
- Non-JSON TTY: show per-VM stepwise progress with concise, single-line completion including outcome (Success|NoOp|Failed|Skipped), total duration, and durable log path.
- JSON stream gains a terminal per-VM BootstrapSummary event with resolved policy (auto|always|skip), effective inputs (ssh host/port/user, remote_dir), durations per step, outcome, and durable log path.

Additional Acceptance
- `--plan` outputs the same BootstrapSummary shape with outcome=Planned and zero durations; exits non-zero on invalid config.
- Per-VM failures include one actionable hint and the durable log path in both TTY and JSON forms.
---

---

