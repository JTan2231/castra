---
Snapshot sync (v0.8.1)

- Trigger: first successful broker handshake following a change in (base image hash, bootstrap artifact hash) idempotence stamp.
- Events: BootstrapStarted → BootstrapCompleted(status: Success|NoOp) | BootstrapFailed; emit via reporter and store step logs with durations.
- Config: knobs to disable or force ("always") globally or per‑VM; safe defaults.
- Behavior: status remains responsive during long runs; safe re‑runs emit NoOp; portability target remains macOS/Linux hosts with POSIX + Nix + SSH.
- Cross‑links: consumes ManagedImageVerificationResult (Thread 10) when present.


---

