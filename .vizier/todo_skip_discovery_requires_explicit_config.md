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

Anchor addition
- src/cli.rs: help text for `--skip-discovery` and `--config` must be updated to describe the stricter semantics and required pairing.

Acceptance clarification
- Commands fail fast without attempting upward directory walking when `--skip-discovery` is present and `--config` is missing; copy includes an example invocation with `--config <path>`.

---

Thread 1 — UX-first CLI contract. Snapshot v0.7.2 reference.

Tension
- `--skip-discovery` still triggers upward search when `--config` is omitted, violating user expectations for explicitness.

Change (product-level)
- When `--skip-discovery` is present without `--config`, commands that require a project config fail fast with a clear usage/config error and actionable guidance. No filesystem walking occurs.

Acceptance criteria
- `castra status --skip-discovery` (no --config) exits with usage/config error; message explains that `--config <path>` is required when discovery is skipped and provides an example.
- Same behavior for `up`, `down`, `ports`, and `logs`.
- With `--skip-discovery --config <path>`, no directory walking is performed (verified by a unit/integration test along the CLI/library path).
- Help text for `--skip-discovery` and `--config` reflects the stricter semantics.

Anchors
- src/app/common.rs; src/core/options.rs; src/cli.rs (flag help/copy).