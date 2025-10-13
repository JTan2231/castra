Snapshot v0.7.4 update — Partially delivered in commit 1583c8c

Status
- CLI help updated and enforcement added to require `--config` when `--skip-discovery` is set (commit 1583c8c). This advances the UX contract.

Remaining scope (product-level)
- Ensure enforcement applies uniformly across subcommands (status, up, down, ports, logs) and the library path used by the CLI.
- Add unit/integration tests that assert: no filesystem walking occurs when `--skip-discovery` is present; missing `--config` (or `--state-root` for clean) yields a usage/config error with actionable guidance.
- Keep copy consistent across commands and include a one-line example (`--config <path>`). Confirm exit codes.
- Cross-link: `castra clean` must adopt the same semantics when introduced (Thread 14).

Acceptance updates
- Verified by tests rather than manual-only; include a smoke test that captures directory walk attempts (e.g., stubbed discovery provider) and asserts zero calls under `--skip-discovery`.
- Help text consistency snapshot for the affected commands.

---

