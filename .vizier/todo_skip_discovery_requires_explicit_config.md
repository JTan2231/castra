Thread links: Thread 1 (UX-first CLI contract), Thread 4 (Project configuration and discovery)

Tension
- Users expect `--skip-discovery` to require an explicit config path. Today it only disables synthetic fallback and still walks the filesystem upward, weakening the contract.

Evidence
- src/app/common.rs:17 delegates to options that still perform upward search when `--config` is omitted.
- src/core/options.rs:37 defines discovery behavior irrespective of the skip flag.

Change (product-level)
- When `--skip-discovery` is set and `--config` is not provided, commands that require a project config (init excluded) must fail fast with a usage/config error and crisp copy indicating that a path is required when discovery is skipped.

Acceptance criteria
- `castra status --skip-discovery` without `--config` exits with the documented exit code (usage or config) and prints actionable guidance.
- `up`, `down`, `ports`, and `logs` exhibit the same behavior.
- With both `--skip-discovery --config <path>`, no directory walking occurs (verified by a targeted unit/integration test exercising the library API path used by CLI).
- Help text for `--skip-discovery` updated to reflect the stricter semantics.
Snapshot reference bumped to v0.7.1. Clarify exit behavior: treat missing --config under --skip-discovery as a usage/config error with guidance; ensure no filesystem walking occurs when the flag is set.

---

