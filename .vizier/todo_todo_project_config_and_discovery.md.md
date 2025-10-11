Update (SNAPSHOT v0.4)

Evidence
- Config parser validates required fields with example-rich errors; unknown fields warned with context; paths resolved relative to config directory.
- Commands emit warnings collected from the parser.

Refinement
- Summarize warnings once per invocation with a count and bullet list before detailed outputs.
- Add suggestion lines pointing to `castra ports` and `castra status` after successful parse when warnings exist.

Acceptance criteria (amended v0.4)
- On parse with warnings, show a short summary block (e.g., `Found 2 warnings:` then bullets) before proceeding. [NEXT]


---

