Thread 12 — Post-boot bootstrap pipeline (Product level)

Why
- After first successful connectivity, operators want automatic, idempotent application of a host-provided bootstrap (e.g., Nix flake) over SSH.

Desired behavior
- Trigger once per VM per image/content hash after a successful broker handshake.
- Steps are logged durably: connect → transfer → apply → verify.
- Emit BootstrapStarted / BootstrapCompleted / BootstrapFailed events with summary fields.
- Idempotence: re-running with unchanged inputs performs no-op and reports that status.

Acceptance criteria
- Idempotence stamps live under state root; subsequent runs skip when unchanged.
- Failure modes are observable with actionable error messages; status remains responsive.
- Pipeline can be disabled via config; defaults are safe.

Anchors
- docs/BOOTSTRAP.md; src/core/status.rs; state-root conventions

Notes
- Keep implementation approach open; acceptance focuses on triggers, events, durability, and idempotence.