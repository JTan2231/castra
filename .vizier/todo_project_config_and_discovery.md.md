
---
Update (SNAPSHOT v0.5)

Evidence
- init scaffolds castra.toml and .castra/; config discovery implemented with upward search and --config override; parser validates with friendly errors, resolves relative paths, and accumulates warnings. Warning summary emitted once per command with next-step hints.

Refinement
- Continue tightening diagnostics with example snippets in any remaining error paths.

Acceptance criteria (amended v0.5)
- Warning summary block present on commands that load config, with count and actionable next steps. [DONE]


---

