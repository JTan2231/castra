Thread: Project configuration and discovery (depends on SNAPSHOT v0.1)

Goal
- Establish a simple, readable project config and discovery rules.

Acceptance criteria
- `castra init` creates a `castra.toml` (or similar) at project root with a minimal single-VM definition and `.castra/` workdir.
- CLI discovers config from CWD upward; override via `--config` path.
- Config validates with friendly errors; unknown fields warned, required fields explained.

Notes
- Keep format open (TOML/YAML/JSON); TOML is implied by Rust ecosystem but not mandated here.
---
Update (SNAPSHOT v0.2)

Evidence
- `castra init` generates castra.toml with a single VM and creates a .castra/ workdir (src/main.rs: handle_init, default_config_contents).
- Config discovery implemented: searches upward for castra.toml; `--config` override respected; `--skip-discovery` supported on several commands.

Refinement
- Add config parsing/validation with user-friendly diagnostics (unknown fields warn; missing required fields explained with examples).
- Ensure paths in generated config are relative to project root and validated on first read with actionable errors.

Acceptance criteria (amended)
- Existing criteria remain, with generated config passing a round-trip parse on first `up`/`status`.
- Discovery errors include the search root and next steps (already present).

---

---
Update (SNAPSHOT v0.3)

Evidence
- Parser performs validation and accumulates non-fatal warnings for unknown fields and suspicious patterns.
- Relative paths in config are resolved against the config directory.

Refinement
- Tighten diagnostic copy: include actionable suggestions and example snippets in error messages for missing/invalid fields.
- Emit a summary block after parse when warnings exist (count + brief bullets), and point to `castra ports`/`status` as next checks.

Acceptance criteria (amended v0.3)
- Unknown fields: clearly warned with path context (e.g., vms[0].foo) and suggestion.
- Relative paths resolved deterministically; errors mention the base directory used.
- On parse with warnings, exit code remains success, but a visible summary is shown once per invocation.
---

---

