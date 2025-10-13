Status — Delivered in v0.7.8

- `--active` mode shipped with stable columns and runtime inspection. Add a short doc snippet and a smoke test verifying STATUS column toggles while headers remain identical.
- Close this thread once docs/tests are merged.

---

Document ports --active and add smoke test for stable columns and status toggle. (thread: ports-active)

Describe a brief doc snippet and a single integration smoke test that demonstrate `castra ports` vs `castra ports --active`, asserting identical headers/column order while the STATUS cell reflects runtime (Active/Inactive). Close thread when merged.

Acceptance Criteria:
- Docs:
  - Add a short example showing `castra ports` (declared view) and `castra ports --active` (runtime view) for the same project, calling out that columns are identical and only STATUS content changes.
  - Note scripting stability: headers/column order are stable across modes.
- Smoke test:
  - With a minimal project and one declared hostfwd, when the VM is running:
    - `castra ports` and `castra ports --active` produce identical headers/column order.
    - The row for the mapping shows STATUS=Active under `--active`.
  - After stopping the VM:
    - The same commands still share identical headers/column order.
    - The row shows STATUS=Inactive under `--active`.
  - Test runs without elevated privileges and completes quickly on CI.
- Copy/help:
  - `castra ports --help` includes a one-line description for `--active` and mentions identical columns.

Pointers:
- src/app/ports.rs (help/copy)
- docs/ (ports section/snippet)
- tests/integration/ports_active.rs