---
Snapshot sync (v0.8.1)

- Align event names and order with Snapshot Thread 2:
  ShutdownRequested → CooperativeAttempted(method: ACPI|QMP|Agent) → CooperativeSucceeded | CooperativeTimedOut → Escalation(SIGTERM)? → Escalation(SIGKILL)? → ShutdownComplete(outcome: Graceful|Forced).
- Clarify acceptance: timeouts configurable via CLI/options; per‑VM isolation; events emitted to logs and JSON surfaces with stable fields; exit codes reflect Graceful vs Forced.
- Notes: mechanism remains open (ACPI/QMP/Agent), but observable sequence and bounded waits are required.
- Cross‑link: supersedes overlapping "graceful_shutdown_acpi_qmp" TODO; treat this as canonical for Thread 2.


---

