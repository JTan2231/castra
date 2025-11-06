Thread 50 — Vizier Removal — Open questions to resolve

- How does UI discover agent runtimes per VM? Options: static project config; harness metadata; VM self-report. Acceptance: a single, documented mechanism with fallback.
- Do we need minimal health checks in core/harness, or is health delegated to UI? Acceptance: clear ownership and a user-visible health surface.
- What replaces Vizier-provided usage accounting? Acceptance: UI surfaces basic per-session usage with a path to deeper metrics.
- Archival policy: retain or purge historical Vizier docs/examples? Acceptance: decision recorded; code/docs reflect it.

Cross-link
- See vizier-removal/IMPLEMENTATION_PLAN.md (Open Questions).