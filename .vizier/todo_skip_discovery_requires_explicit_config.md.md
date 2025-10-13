Update — Snapshot v0.7.8

- Extend strict `--skip-discovery` semantics to new `bus` subcommands. Acceptance: `castra bus publish|tail --skip-discovery` without an explicit `--config` (or `--state-root` where applicable) fails fast with clear guidance; no filesystem walking occurs. Help text for `bus` subcommands mirrors the global flag semantics.
- CLEAN alignment reiterated: `castra clean` must honor the same pairing requirement (`--config` or `--state-root`) when `--skip-discovery` is present.

---

