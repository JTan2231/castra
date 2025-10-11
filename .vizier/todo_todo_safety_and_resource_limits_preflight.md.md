Update (SNAPSHOT v0.4)

Evidence
- Preflight in `up` checks: qemu-system presence, port conflicts in config, runtime port availability (bind tests), overlay dir creation, and overlay creation via `qemu-img` when missing; produces actionable errors.

Refinement
- Add disk-space check for overlay/log directories with configurable floor (e.g., warn <2 GiB, fail <500 MiB). 
- Add host CPU/mem headroom checks vs requested totals; allow `--force` override in `up`.
- Add global signal handlers to translate SIGINT/SIGTERM into graceful `down` for managed processes when appropriate.

Acceptance criteria (amended v0.4)
- Disk/memory/CPU headroom checks exist with clear thresholds and messages; `--force` bypass supported. [NEXT]


---

