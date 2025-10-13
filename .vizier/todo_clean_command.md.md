Follow-ups — v0.7.8

- Add README/CLEAN.md examples demonstrating reclaimed byte totals and overlay safeguards.
- Integration tests to validate: accurate byte totals, permission downgrade behavior in global mode, and managed-image remediation path documented in CLEAN.md.
- Ensure skip-discovery contract mirrored: `--skip-discovery` without `--config`/`--state-root` fails fast with crisp diagnostics; add tests.

---

Document clean reclaimed-bytes and safeguards; add integration tests for totals, permissions, and skip‑discovery pairing. (thread: first-class clean — snapshot v0.7.8/Thread 14)

Describe examples in README/CLEAN.md that show reclaimed byte totals and overlay safeguards, and add integration tests verifying byte accounting accuracy, permission downgrade handling in global mode, the managed‑image remediation path, and strict skip‑discovery pairing.

Acceptance Criteria:
- Docs:
  - README and CLEAN.md include examples for workspace and --global runs that display reclaimed byte totals and explicitly call out overlay safeguards (overlays only removed with --include-overlays).
  - CLEAN.md documents the managed‑image remediation path (e.g., use --managed-only) and references expected byte accounting.
- Skip‑discovery contract:
  - `castra clean --skip-discovery` without --config or --state-root fails fast with a clear diagnostic; tests cover failure and success when correctly paired. Copy aligns with Thread 1 semantics.
- Integration tests:
  - Byte totals: create known-size files; `--dry-run` and actual run report accurate reclaimed totals in human and JSON outputs.
  - Permission downgrade (global mode): unreadable/unwritable entries produce graceful diagnostics (no panic) and appropriate exit status; partial progress allowed per CLEAN.md policy.
  - Overlay safeguard: overlays are preserved by default and only removed when `--include-overlays` is set; verified via assertions.
  - Managed-image remediation: invoking `castra clean --managed-only` removes managed caches and logs CleanupProgress events; subsequent managed-image use succeeds.
- Observability:
  - CleanupProgress events (path, kind, bytes, dry_run) are emitted and visible via OperationOutput/logs with stable field names.

Pointers:
- docs/CLEAN.md; README (examples)
- src/app/clean.rs; src/cli.rs (help/copy)
- tests/integration/clean_*.rs (byte totals, permissions, skip‑discovery, overlays, managed-only)
- src/core/operations/clean.rs; src/core/project.rs (event/byte accounting surfaces)