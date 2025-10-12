
---
Evidence update (2025-10-12):
- --version prints Cargo-only semver (src/cli.rs:10); no build.rs embeds git SHA. docs/ lacks MSRV or release workflow. Anchors Thread 9.

Acceptance refinement:
- Embed short SHA when available; document MSRV and release steps under docs/. --version must degrade gracefully when metadata missing.


---

