
---
Update (SNAPSHOT v0.5)

Evidence
- --version surfaces Cargo package version only. MSRV and release docs remain TBD.

Refinement
- Add optional git short SHA to --version when available; document MSRV and a minimal RELEASING.md.

Acceptance criteria (amended v0.5)
- `castra --version` shows semver and `(git <shortsha>)` when available. [OPEN]
- MSRV noted in docs; basic release checklist exists. [OPEN]


---

