
---
Update (SNAPSHOT v0.5)

Evidence
- Commands implemented: init, up, down, status, ports, logs (plus hidden broker). Empty invocation prints help and exits 64. Help/version exit 0. Exit codes mapped consistently via CliError.
- ports, status, logs copy matches implemented behavior; warning summary block appears once per command when parser warns.

Refinement
- Keep per-command help in lockstep with behavior (e.g., status legend and logs prefixes). Ensure examples in help stay accurate.

Acceptance criteria (amended v0.5)
- Maintain exit-code policy as features land. [ONGOING]
- Help text accurately describes broker column semantics (waiting/offline; reachable pending handshake).
- No dangling references to non-existent files/paths in help or error copy. [DONE]


---

