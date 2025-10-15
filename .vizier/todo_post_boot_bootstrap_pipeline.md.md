---
Update (Snapshot v0.8.0 alignment)

- Trigger clarified: first successful broker handshake since current image/content hash change.
- Events naming alignment: BootstrapStarted / BootstrapCompleted / BootstrapFailed must be emitted via reporter, coexisting with lifecycle and managed-image events.
- Idempotence stamp must include flake (or artifact) content hash and guest base image hash; if unchanged, emit BootstrapCompleted(status: NoOp).
- Config: add product-level knob to disable or force ("always") bootstrap per VM or globally.
- Acceptance: logs under state root include step markers with durations; status remains responsive during long-running apply.
- Cross-links: consumes ManagedImageVerificationResult (Thread 10) where available to verify base before application.

---

---
Additions
- Portability expectation: bootstrap flow relies on POSIX shell + Nix + SSH; must work on macOS and Linux hosts.
- Examples/docs: provide reference scripts (bootstrap-host.sh, bootstrapd.sh, guest-bootstrap.sh) and wiring via [workflows].init in castra.toml as non-normative examples.
- Observability: store logs under <state_root>/logs/bootstrap/ with step-level durations; failed runs leave markers for inspection.


---

