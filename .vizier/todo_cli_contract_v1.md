Thread: UX-first CLI contract (depends on SNAPSHOT v0.1, code state no CLI)

Goal
- Provide a friendly CLI skeleton with subcommands and helpful output.

Acceptance criteria
- `castra --help` shows concise overview with subcommands: init, up, down, status, ports, logs, version.
- `castra --version` prints version and commit (commit optional until VCS hooked).
- All subcommands exist and return sensible non-zero exit code when unsupported/NYI, with a hint to the roadmap.
- Exit codes are consistent (0 for success, 64+ for usage errors).

Notes
- Keep implementation open: clap/argparse choice is not mandated; prioritize UX copy and structure.
---
Update (SNAPSHOT v0.2)

Evidence
- clap-based CLI exists with subcommands; help/version wired; exit codes differentiated in main.rs.
- NYI commands emit structured errors with a tracking path pointing to .vizier/* (mismatch with repo TODO locations).

Refinement
- Adjust NYI tracking hints to reference existing TODO filenames at project root (without .vizier/ prefix) to avoid confusion.
- Ensure `castra --help` shows concise subcommand summaries matching current behavior and hints for NYI commands.

Acceptance criteria (amended)
- NYI errors reference real TODO files in this repo.
- `castra` with no subcommand prints help and exits with usage error code (64) [already true].
- Copy review pass for help text to align with project promise (UX-first, legible).

---

---
Update (SNAPSHOT v0.3)

Evidence
- cli.rs long_about now references repo-root todo_*.md files (no `.vizier/` mention).
- Commands include: init, up, down, status, ports, logs; help/version work; NYI surfaces point to real TODOs.
- Ports command copy highlights conflicts and broker reservations.

Refinement
- Do a per-command help copy pass to ensure summaries match current behavior precisely and are consistent in tone.
- Reconfirm exit code consistency across commands as new features land.

Acceptance criteria (amended v0.3)
- Long help and NYI hints reference correct repo-root docs. [DONE]
- `ports` help mentions conflict detection and broker-port overlap. [DONE]
- Consistent exit codes maintained as features ship. [ONGOING]
---

---

