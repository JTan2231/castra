Update (SNAPSHOT v0.4)

Evidence
- CLI now implements functional subcommands: init, ports, up, down, status, logs; hidden `broker` used internally by `up`.
- Exit code policy enforced via CliError::exit_code; `--help`/`--version` exit 0; usage errors exit 64; other failures mapped to 65–74.
- Help/long_about copy references repo-root TODOs; per-command help aligned with behavior.

Refinement
- Do a final copy pass to ensure per-command descriptions reflect current behavior (e.g., `logs` tail/follow semantics, `ports` conflict/broker notes).
- Add examples section to `--help` (e.g., `castra ports --verbose`, `castra logs --tail 100 --follow`).

Acceptance criteria (amended v0.4)
- All implemented commands have accurate, consistent help text and examples. [ONGOING]
- Exit codes remain consistent as features evolve. [ONGOING]


---

