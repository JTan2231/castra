Snapshot ref bump: v0.7.2. Acceptance clarifications and copy hooks.

- Exit behavior: use the existing CLI usage/config error path; exit code matches other argument validation failures.
- Help text: `--skip-discovery` now states "requires --config <path>" explicitly; include one-liner example.
- Tests: add a focused unit/integration test that asserts no filesystem walking when flag is set without config (expect immediate error) and that walking is suppressed when both are set.


---

