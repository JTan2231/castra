Delivery note — Snapshot v0.7.6

- Status: Shipped. `castra clean` implemented with workspace/global scopes, dry-run, managed-only, include-* toggles, running-process safeguards with `--force`, events (`CleanupProgress`) and reclaimed-bytes accounting. Honors Thread 1 skip-discovery contract (`--config` or `--state-root` required when skipping).
- Evidence: commit 9eea08b (feat(clean): add first-class clean command). Anchors touched: src/cli.rs; src/app/clean.rs; src/core/operations/clean.rs; src/core/project.rs; src/core/runtime.rs.

Residuals / follow-ups
- Docs: Expand CLEAN.md/README with examples and remediation flows (checksum mismatch → clean managed cache).
- Byte accounting: add an integration test asserting reclaimed totals across logs/handshakes/overlays toggles.
- Permissions: add an explicit regression test that permission errors in global mode downgrade to diagnostics and do not abort the run.
- Telemetry: ensure CleanupOutcome is surfaced in machine-readable OperationOutput for embedders (documented in docs/library_usage.md).


---

