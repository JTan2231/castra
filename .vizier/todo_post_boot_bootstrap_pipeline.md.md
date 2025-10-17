---
Update (stamp-free pipeline locked; rerun semantics clarified)

Status
- Host-side idempotence stamps are removed. Reruns are governed by user policy and runtime signals, not persisted stamps. JSON schemas remain unchanged.

Scope adjustments
- Replace previous Section 4 (Idempotence stamp persistence and reruns) with:
  4) Reruns without stamps (policy-driven)
     - `auto`: Attempt bootstrap after first successful broker handshake each `up` invocation; whether the script performs a NoOp is decided by script/verify logic (e.g., `Castra:noop` sentinel) rather than host stamps.
     - `always`: Force running the pipeline irrespective of prior runs; still honors script-level NoOp sentinel when appropriate.
     - `skip`: Do not run bootstrap; surface as Skipped with rationale.
     - Per-VM overrides continue to take precedence; conflicts are rejected preflight.

Acceptance deltas
- No stamp reads/writes. Repeated `up` in `auto`/`always` re-attempts the pipeline; NoOp recognized via script sentinel or verify logic; durable per-VM logs are still written.
- Documentation updates must explicitly state that there is no host-side persistence controlling idempotence; point users to script-level idempotence.

Unchanged items (still in scope)
- `--plan` dry-run with deterministic, side-effect-free summaries.
- TTY progress lines and concise completion summaries with hints; JSON output unchanged.
- Actionable error mapping with durable log paths.

Docs/Help
- Update examples in docs/BOOTSTRAP.md and CLI help to remove stamp language and show `auto|always|skip` behaviors and `Castra:noop` sentinel.
---

---

