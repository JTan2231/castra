---
Update (SNAPSHOT v0.3)

Evidence
- main.rs: NYI errors now reference repo-local TODOs (e.g., todo_qemu_lifecycle_minimal.md, todo_observability_and_status_copy.md).
- cli.rs: long_about still references a `.vizier/` directory which does not exist in this repo.
- `ports` subcommand implemented with helpful table output and warnings sourced from config loader.

Refinement
- Replace `.vizier/` mention in long_about with guidance to root-level TODOs or project README.
- Add brief subcommand summaries in `--help` that match actual behaviors (e.g., `ports` explains declared vs. conflicts and broker overlap).

Acceptance criteria (amended)
- `castra --help` long_about references correct docs (no `.vizier/`).
- `ports` help text mentions conflict detection and broker-port overlap note.
- All NYI errors continue to reference real TODO files.


---

