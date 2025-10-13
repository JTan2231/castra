Scope update — Snapshot v0.7.7

- Bus CLI added (`castra bus publish|tail`) and wired through the same config load path. These subcommands inherit the strict `--skip-discovery` semantics: when set, they require an explicit `--config <PATH>` and must not walk parent directories.

Acceptance additions
- `castra bus publish --skip-discovery` and `castra bus tail --skip-discovery` without `--config` fail fast with usage/config errors and actionable guidance; with both flags, no filesystem walking occurs (exercise via the same unit/integration harness used by other commands).

Anchors
- src/cli.rs (bus args help copy); src/app/bus.rs (config_load_options usage); src/core/options.rs (ConfigLoadOptions).

---

