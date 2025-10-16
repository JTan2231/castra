
---
Change directive (2025-10-16): Remove all stamp persistence for bootstrap.

Summary
- Eliminate host-side stamp files/state and any stamp-based idempotence checks from the bootstrap pipeline. Bootstrap behavior must be stateless with respect to host persistence.

Scope adjustments (product level)
- CLI modes remain but semantics change:
  - `--bootstrap=auto`: After first successful broker handshake per VM, attempt bootstrap unconditionally (no stamp gating). The bootstrap runner itself may determine NoOp based on live state, but Castra does not persist or consult stamps.
  - `--bootstrap=always`: Same as auto for Castra; preserves user intent signaling but without stamp semantics.
  - `--bootstrap=skip`: Do not run bootstrap.
- `--plan` continues to produce a deterministic summary, but rationales must not reference stamps. Use observable preconditions only (e.g., handshake readiness, artifact presence). If the bootstrap runner can expose an up-front “would change” signal without side effects, surface it; otherwise, plan states “Will attempt after handshake.”
- Progress/UI unchanged, but outcome reasons drop any mention of stamps. Possible outcomes: Success, NoOp (as reported by the runner), Skipped, Failed.

Acceptance updates
- No reads/writes under any state-root path for bootstrap stamps. Cold and warm runs behave identically from Castra’s perspective.
- Re-running `up` with identical inputs still attempts bootstrap in auto/always; if the runner is idempotent, it reports NoOp without host-side persistence.
- Per-VM overrides continue to apply; conflict detection remains, but messages exclude stamp language.
- JSON schema remains unchanged; field values no longer include stamp-related reasons or paths.

Removal task (pointer level)
- Purge stamp code paths and docs:
  - Remove any state-root interactions specific to bootstrap stamps (read/write/delete) and their conditionals.
  - Delete/ignore legacy stamp directories/files on disk without erroring; do not recreate them.
  - Update docs/BOOTSTRAP.md and CLI help to remove stamp concepts and examples.
  - Adjust tests: drop stamp persistence tests; add tests that verify no filesystem stamp writes occur and that repeated runs do not consult stamps.

Non-goals
- Do not introduce alternative persistence or caches for bootstrap decisions at this time.

Notes (safety/correctness)
- Ensure removal does not regress failure observability: durable run logs remain, but these are not “stamps.” Concurrency semantics unchanged.
---

---

