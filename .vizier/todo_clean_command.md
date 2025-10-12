# Thread 14 — First-class `castra clean`

Context
- Source: CLEAN.md defines the UX, scope, and safety requirements for a new clean subcommand.
- Depends on: Thread 1 (skip-discovery contract), Thread 2 (lifecycle safety), Thread 10 (managed images cache semantics).
- Anchors: `src/cli.rs`, `src/app/*`, `src/core/options.rs`, `src/core/operations/clean.rs`, `src/core/status.rs`, `src/core/runtime.rs`, `src/core/project.rs`.

Product outcome
- Users can run `castra clean` to purge caches and workspace ephemera safely, with `--global` and workspace-scoped modes, and with dry-run and force toggles.
- Library/API consumers can invoke the same cleanup via core operations (no CLI coupling).

Acceptance criteria
- CLI: `castra clean --help` lists flags per CLEAN.md (global/workspace selectors, --dry-run, --force, --include-overlays, --include-logs, --include-handshakes, --managed-only, --state-root, --skip-discovery).
- Workspace resolution mirrors existing commands; when `--skip-discovery` is set without `--config`/`--state-root`, the command fails fast with clear diagnostics (Thread 1 alignment).
- Safety: refuses to delete if any VM/broker is running unless `--force`. Diagnostics reference `castra down` and the `--force` escape hatch (Thread 2 alignment).
- Execution plans and results are observable as Events (e.g., `CleanupProgress { path, kind, bytes, dry_run }`) and summarized in human-readable output with reclaimed byte totals.
- Global mode never deletes overlays; workspace mode deletes overlays only with `--include-overlays`.
- Managed images cache, logs, handshakes, and pidfiles are cleaned according to flags, with accurate byte accounting and robust error handling.
- Tests exist covering CLI parsing, dry-run correctness, running-process guard, overlay opt-in, and a tempdir smoke test via the public API.

Notes
- Add `project::default_projects_root()` helper for global sweep. Ensure permission errors in global mode downgrade to diagnostics, not panics.
- Cross-links: surfaced in README/docs as the supported remediation path for checksum mismatch errors (Thread 10).