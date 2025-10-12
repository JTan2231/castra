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

