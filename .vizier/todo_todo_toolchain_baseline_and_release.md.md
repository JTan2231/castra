Update (SNAPSHOT v0.4)

Evidence
- No changes to version surfacing or releasing in this iteration; CLI uses Cargo version via clap.

Refinement
- Keep plan to append git short SHA to `--version` output when available.
- Add RELEASING.md with a simple checklist (tag, changelog, cargo publish or release artifacts).

Acceptance criteria (unchanged)
- `castra --version` optionally includes git SHA; MSRV documented; RELEASING.md present. [NEXT]


---

