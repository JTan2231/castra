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
- Cross-links: surfaced in README/docs as the supported remediation path for checksum mismatch errors (Thread 10).Finalize docs and integration tests for first-class clean.
Ensure `castra clean` is documented with examples and verified by tests covering byte accounting, permission downgrades in global mode, skip‑discovery pairing, and safety guards, aligning with CLEAN.md. Close the thread when docs/tests land. (thread: first-class clean — snapshot v0.7.8/Thread 14)

Acceptance Criteria:
- Docs:
  - README/CLEAN.md include examples for workspace and `--global` modes, showing `--dry-run`, `--force`, `--include-overlays`, `--include-logs`, `--include-handshakes`, `--managed-only`, `--state-root`, and `--skip-discovery`.
  - Help text and docs explicitly state that `--skip-discovery` must be paired with `--config` (or `--state-root` for clean-only state) and show the failure message when omitted.
  - Cross-link from managed image remediation guidance to `castra clean` with a concrete example.
- Tests:
  - Byte totals: an integration test deletes known-sized files and asserts reclaimed byte totals in human output and JSON.
  - Permission downgrade (global mode): test simulates unreadable/unwritable entries and asserts graceful diagnostics without panic; command exits successfully when appropriate.
  - Running-process guard: test asserts refusal while VM/broker running; `--force` overrides and proceeds.
  - Overlay opt-in: test asserts overlays are untouched by default and removed only with `--include-overlays`.
  - Skip-discovery pairing: tests assert fast‑fail when `--skip-discovery` is passed without `--config`/`--state-root`, and success when correctly paired.
  - Public API smoke: tempdir-backed project cleaned via core operation (no CLI), emitting CleanupProgress events and returning a structured outcome.
- Observability:
  - CleanupProgress events (path, kind, bytes, dry_run) are visible in OperationOutput and logs; field names stable for scripts.

Pointers:
- docs/CLEAN.md; README (examples)
- src/app/clean.rs; src/cli.rs (help/copy)
- tests/integration/clean_*.rs (byte totals, permission downgrade, guards, skip‑discovery)
- src/core/operations/clean.rs; src/core/project.rs (API surfaces)