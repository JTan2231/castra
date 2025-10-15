
---
Refinement (examples + integration tests)
- Examples in CLEAN.md and README showing typical reclaim flows and permission downgrade messaging; include --force semantics.
- Integration tests for byte totals and permission downgrade behavior; ensure consistent units/rounding in output.
- Pairing tests with skip-discovery: clean with --state-root works without config; without either flag, exits with code 2 and actionable error.
Cross-links: Thread 14 in snapshot (First-class clean) and Thread 1 (Skip discovery).

---

