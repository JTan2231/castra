Thread 14 — First-class clean: docs/examples and integration tests (Snapshot v0.7.8)

Context
- `castra clean` shipped with scopes, safeguards, CleanupProgress events, and reclaimed-bytes accounting. Docs and tests need to anchor the contract and cover edge cases.

Product change
- Add README/CLEAN.md examples for reclaimed byte totals and overlay safeguards; document managed-image remediation path.
- Add integration tests for byte totals accuracy, permission downgrade behavior (global mode), overlay safeguard, managed-only remediation, and strict skip-discovery pairing.

Acceptance Criteria
- Docs:
  - README and CLEAN.md show workspace and `--global` examples with reclaimed byte totals; explicitly call out overlay safeguards (`--include-overlays`).
  - CLEAN.md documents managed-image remediation path (`--managed-only`) and references byte accounting expectations.
- Skip-discovery contract:
  - `castra clean --skip-discovery` without `--config` or `--state-root` fails fast with clear diagnostics; tests cover failure and success when paired correctly.
- Integration tests:
  - Byte totals: known-size files; `--dry-run` and actual run report accurate totals in human and JSON outputs.
  - Permission downgrade (global mode): unreadable/unwritable entries produce graceful diagnostics and appropriate exit status; partial progress allowed per policy.
  - Overlay safeguard: overlays preserved by default; only removed when `--include-overlays` is set.
  - Managed-only remediation: `castra clean --managed-only` removes managed caches and logs CleanupProgress; subsequent managed-image use succeeds.
- Observability:
  - CleanupProgress events (path, kind, bytes, dry_run) visible via OperationOutput/logs; field names stable.

Pointers
- docs/CLEAN.md; README
- src/app/clean.rs; src/cli.rs (help/copy)
- tests/integration/clean_*.rs
- src/core/operations/clean.rs; src/core/project.rs