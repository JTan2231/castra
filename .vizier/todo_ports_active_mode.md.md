Delivery note — Snapshot v0.7.6

- Status: Shipped. `castra ports --active` implemented with runtime inspection, reasons for inactivity, and stable columns. Tests added and new types re-exported.
- Evidence: commit 003c767 (feat(ports): active view inspects runtime + reasons). Anchors touched: src/core/ports.rs; src/app/ports.rs; src/core/runtime.rs; CLI help.

Residuals / follow-ups
- Performance guardrail: keep <200ms target documented; add a perf smoke test in CI for small projects when feasible.
- Degradation copy: ensure a single inline note on inspection unavailability remains consistent across locales/term widths.
- Scripting contract: add a doc snippet with jq examples confirming column stability.


---

