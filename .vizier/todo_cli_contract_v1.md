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
