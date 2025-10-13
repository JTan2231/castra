Thread 6 — Ports: document `--active` mode and add a smoke test (Snapshot v0.7.8)

Status
- `castra ports --active` shipped with stable columns and runtime inspection.

Change
- Add a brief docs snippet and a single integration smoke test to verify that headers/column order are identical between `ports` and `ports --active`, and that STATUS toggles Active/Inactive with the VM running/stopped.

Acceptance Criteria
- Docs:
  - Example showing `castra ports` vs `castra ports --active` for the same project; call out identical headers/column order and only STATUS changes.
  - `castra ports --help` mentions identical columns and explains `--active`.
- Smoke test:
  - When VM is running: headers identical; row shows STATUS=Active under `--active`.
  - After stopping: headers identical; row shows STATUS=Inactive under `--active`.
  - Runs without elevated privileges; suitable for CI.

Pointers
- src/app/ports.rs (help/copy)
- docs/ (ports section/snippet)
- tests/integration/ports_active.rs