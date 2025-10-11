---
Update (SNAPSHOT v0.3)

Evidence
- src/config.rs: Loader performs schema validation, warns on unknown fields, resolves relative paths against the config directory, validates port forward entries, and surfaces warnings (e.g., duplicate guest-port forwards per VM). Broker port default set (7070) and collisions detectable via `port_conflicts()`.
- `ports` command consumes loader output and surfaces warnings to the user before printing.

Refinement
- Improve error copy with short examples on common failures (missing `[project]`, missing `[[vms]]`, bad memory units). Ensure messages include the config filename and suggest `castra init` for scaffolding.
- When resolving paths, include a tip if `.castra/` directory is missing, pointing to `castra init` or mkdir.

Acceptance criteria (amended)
- Error messages for validation failures include: field name, example of a valid value, and config path.
- First invocation of `up`/`ports` after editing invalid config yields actionable guidance without stack traces.


---

