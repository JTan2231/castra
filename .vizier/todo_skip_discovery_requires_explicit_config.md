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
- src/app/common.rs; src/core/options.rs; src/cli.rs (flag help/copy).Cross-link: CLEAN.md mandates that `castra clean` obey the same skip-discovery contract. Acceptance: when `--skip-discovery` is used without `--config` or `--state-root`, both `status` and `clean` fail fast with a clear diagnostic (no filesystem walking).

---

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
- src/app/common.rs; src/core/options.rs; src/cli.rs (flag help/copy).Cross-link: CLEAN.md mandates that `castra clean` obey the same skip-discovery contract. Acceptance: when `--skip-discovery` is used without `--config` or `--state-root`, both `status` and `clean` fail fast with a clear diagnostic (no filesystem walking).

---
Enforce strict skip-discovery pairing across all commands (status/up/down/ports/logs/bus/clean) with no filesystem walking.
When --skip-discovery is set without an explicit project path, commands that require config must fail fast with a clear usage/config error; when paired with --config (or --state-root for clean-only), commands perform zero directory walking. Update help/legend to reflect stricter semantics. (thread: discovery-semantics — snapshot v0.7.8/Thread 1)

Acceptance Criteria:
- Fast-fail behavior:
  - `castra status --skip-discovery` without `--config` exits with the documented usage/config code and prints actionable guidance including an example (`--config <path>`).
  - Same fast-fail applies to `up`, `down`, `ports`, `logs`, and `bus` subcommands when `--skip-discovery` is present without `--config`.
  - `castra clean --skip-discovery` fast-fails unless paired with `--config` or `--state-root`, with diagnostics explaining the required pairing.
- No-walk guarantee:
  - With `--skip-discovery --config <path>`, and for `clean` with `--state-root`, no upward filesystem walking occurs; verified via targeted tests that exercise the library API path used by the CLI.
- Help and docs:
  - Help text for `--skip-discovery`, `--config`, and `--state-root` updated to describe required pairing and stricter semantics; include one example per relevant command.
- Tests:
  - Integration/unit tests cover fast-fail for each command (`status`, `up`, `down`, `ports`, `logs`, `bus`, `clean`) when `--skip-discovery` lacks a required path.
  - Positive-path tests confirm zero discovery when correctly paired and validate exit codes/messages.
  - Tests include the bus and clean surfaces explicitly.

Pointers:
- src/app/common.rs; src/core/options.rs (discovery enforcement)
- src/cli.rs; src/app/* (help/copy per command)
- tests/integration/ (skip-discovery pairing and no-walk)Finalize strict skip-discovery contract with help copy and tests (no filesystem walking).
When `--skip-discovery` is provided, commands that require a project config must either be paired with `--config <path>` (or `--state-root` for clean-only) or fail fast with a clear usage/config diagnostic. Confirm zero directory walking on correctly paired invocations. (thread: discovery-semantics — snapshot v0.7.9/Thread 1)

Acceptance Criteria
- Fast-fail behavior (verified):
  - `castra status --skip-discovery` without `--config` exits with the documented usage/config code and prints guidance including an example (`--config <path>`).
  - Same fast-fail for `up`, `down`, `ports`, `logs`, and `bus` when `--skip-discovery` lacks `--config`.
  - `castra clean --skip-discovery` fast-fails unless paired with `--config` or `--state-root`, with diagnostics explaining the required pairing.
- No-walk guarantee (verified):
  - With `--skip-discovery --config <path>`, and for `clean` with `--state-root`, no upward filesystem walking occurs; tests exercise the library path used by the CLI.
- Help and docs:
  - Help text for `--skip-discovery`, `--config`, and `--state-root` updated to describe required pairing and stricter semantics; include one example per relevant command.
- Tests:
  - Integration/unit tests cover fast-fail for each command (`status`, `up`, `down`, `ports`, `logs`, `bus`, `clean`) when `--skip-discovery` lacks a required path.
  - Positive-path tests confirm zero discovery when correctly paired and validate exit codes/messages.

Pointers
- src/app/common.rs; src/core/options.rs (discovery enforcement path)
- src/cli.rs; src/app/* (help/copy)
- tests/integration/* (skip-discovery pairing and no-walk)