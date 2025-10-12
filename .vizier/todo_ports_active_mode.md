# Thread 6 — Networking and connectivity ergonomics
Snapshot: v0.7 (Current)

Goal
- Add a runtime view to contrast declared vs active forwards: `castra ports --active`.

Why (tension)
- Snapshot Thread 6: we promise clarity on declared vs active. Today we show declared + conflicts, but not what QEMU actually activated.

Desired behavior (product level)
- `castra ports` continues to show the declared table by default.
- `castra ports --active` augments or replaces STATUS to reflect runtime state for running VMs (active/inactive), with clear copy when VMs are stopped.
- Host/broker collisions and duplicates remain highlighted.

Acceptance criteria
- When VM(s) are running, `castra ports --active` shows active status for hostfwd entries QEMU has established; for stopped VMs, entries show inactive with a hint.
- The command runs without elevated privileges and completes quickly (<200ms for small projects).
- Output remains stable and scripts friendly; columns/labels are consistent.

Scope and anchors (non-prescriptive)
- Anchors: src/core/ports.rs (summary), src/app/ports.rs (rendering). Runtime inspection may use QMP, QEMU monitor output, or process args/logs — keep choice open.
Snapshot reference bumped to v0.7.1. Preserve acceptance and anchors; emphasize that summarize() must be able to emit Active where applicable and CLI adds --active flag with consistent columns.

---

Anchors refinement
- src/core/ports.rs: enum includes `Active`, but `summarize()` never produces it; adjust summary path to emit `Active` where applicable.
- src/cli.rs: introduce `--active` flag under `ports` subcommand; keep columns stable across modes.

Acceptance addition
- `castra ports --active` exit code and output format are stable for scripting; columns/headers do not change order vs default mode (content of STATUS column changes to reflect Active/Inactive).

---

Thread 6 — Networking and connectivity ergonomics. Snapshot v0.7.2 reference.

Tension
- Users cannot view runtime-active port bindings; summarize() never yields Active and CLI lacks a flag.

Change (product-level)
- Add `--active` mode to `ports` that inspects runtime to classify each declared mapping as Active/Inactive; keep columns identical to default view. STATUS cell varies only.
- Degrade gracefully with an inline note if backend inspection is unavailable.

Acceptance criteria
- `castra ports` (default) shows Declared view identical to today.
- `castra ports --active` shows same columns; STATUS values become Active/Inactive with a short reason if available.
- End-to-end latency target <200ms for small projects; if exceeded or unsupported, a single note explains fallback without failing.
- Help text documents scripting stability: columns stable across modes.

Anchors
- src/core/ports.rs (summarize); src/app/ports.rs or src/app/mod.rs (CLI flag/help); src/core/runtime.rs for inspection hook.