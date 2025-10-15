Thread 2 — Cooperative shutdown lifecycle (Product level)

Why
- Current down path skips guest-cooperative phase; users need predictable, observable shutdown with bounded waits.

Desired behavior
- Before host termination, attempt a guest-cooperative shutdown. Emit ordered events per VM:
  ShutdownRequested → GuestCooperativeAttempted → GuestCooperativeConfirmed | GuestCooperativeTimeout → HostTerminate → HostKill (only if needed) → ShutdownComplete.
- Timeouts are configurable via CLI/opts. Status remains responsive throughout.
- Exit codes distinguish clean cooperative shutdown vs forced kill.

Acceptance criteria
- Events appear in per-VM logs and machine-parseable output with stable names and fields.
- With a responsive guest, sequence includes GuestCooperativeConfirmed and no HostKill.
- With an unresponsive guest past timeout, sequence includes GuestCooperativeTimeout and HostTerminate; HostKill only when termination fails.
- CLI help describes the lifecycle and timeouts.

Anchors
- src/core/runtime.rs; src/core/events.rs; src/core/options.rs; src/app/down.rs

Notes
- Keep implementation open (ACPI/QMP/etc.) so long as observable behavior and events match acceptance.