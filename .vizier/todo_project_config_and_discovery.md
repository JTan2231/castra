Thread: Project configuration and discovery (depends on SNAPSHOT v0.1)

Goal
- Establish a simple, readable project config and discovery rules.

Acceptance criteria
- `castra init` creates a `castra.toml` (or similar) at project root with a minimal single-VM definition and `.castra/` workdir.
- CLI discovers config from CWD upward; override via `--config` path.
- Config validates with friendly errors; unknown fields warned, required fields explained.

Notes
- Keep format open (TOML/YAML/JSON); TOML is implied by Rust ecosystem but not mandated here.
