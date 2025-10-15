
---
Refinement (acceptance criteria + surfaces)
- Scope: status, up, down, ports, logs, bus, clean.
- Behavior: when `--skip-discovery` is set, the process performs zero filesystem scanning of hosts/projects; it must require either `--config <file>` for all commands except `clean`, or `--state-root <dir>` for `clean`.
- Help/UX: `--help` for each affected command mentions the stricter contract and shows examples.
- Exit codes: missing-required-flag exits with code 2 and prints a single-line actionable error.
- Tests:
  - Asserts no stat/walk on $PWD or parent trees (use a temporary dir and an injected sentinel path to detect any walk attempts).
  - Verify bus/clean subcommands adhere to the same contract.
  - JSON status path remains non-blocking and still emits values when config is provided explicitly.
- Docs: CLEAN.md and README flag pairing examples; BUS.md call-outs for bus surfaces.
Cross-links: Thread 1 in snapshot (Skip discovery).

---

