
---
Update (stamp-free alignment)
- Remove host-side idempotence stamp persistence. Rerun semantics are policy-driven only: auto|always|skip. Idempotence comes from script-level NoOp sentinel or verify step, not stamps.

Revised Scope
1) CLI affordances remain: `--bootstrap=auto|always|skip` with per-VM CSV overrides and `--plan` dry-run.
2) Human-friendly TTY progress and concise completion summaries; JSON schema unchanged.
3) Actionable errors with durable log paths and 3–5 friendly hints.
4) Rerun semantics (no stamps):
   - auto: attempt per run; script may short-circuit via NoOp/verify.
   - always: force execution every run; script may still short-circuit internally.
   - skip: never run; surface Skipped with rationale. Preflight rejects conflicting per-VM overrides.

Revised Acceptance
- `castra up --bootstrap=auto|always|skip` with per-VM overrides behaves as above; conflicts fail preflight.
- `--plan` emits deterministic per-VM intent and resolved inputs; exit 0 when valid; no side effects.
- Non-JSON TTY shows compact progress + final one-liners; `--json` unchanged.
- Failures show a concise hint + durable log path.
- No host-side stamp reads/writes; repeated runs follow policy; outcomes are returned in input order; status stays responsive.

Notes
- Docs/help must explicitly state that bootstrap is stamp-free; idempotence is achieved by the runner.
---

---

