
---
Update (SNAPSHOT v0.5)

Evidence
- init scaffolds castra.toml and .castra/; config discovery implemented with upward search and --config override; parser validates with friendly errors, resolves relative paths, and accumulates warnings. Warning summary emitted once per command with next-step hints.

Refinement
- Continue tightening diagnostics with example snippets in any remaining error paths.

Acceptance criteria (amended v0.5)
- Warning summary block present on commands that load config, with count and actionable next steps. [DONE]


---


---
Alignment with Seamless Alpine Bootstrap (Thread 10)
- Incorporate a default-project branch when discovery fails: `load_or_default_project` returns an in-memory config referencing the managed Alpine image (`alpine-minimal@v1`).
- Ensure the fallback path preserves current CLI exit codes and warning-summary semantics; no files are written to disk unless assets are fetched.
- Acceptance hook: `castra up` in an empty directory proceeds without `ConfigDiscoveryFailed` and launches the managed VM when network/cache permits.


---

