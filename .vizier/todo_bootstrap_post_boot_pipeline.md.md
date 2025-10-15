
---
Refinement (idempotence + observability)
- Pipeline triggers after fresh handshake; applies Nix flake over SSH.
- Idempotence via hash stamps; re-apply only when profile hash changes.
- Observable logs under state root with terse CLI summary; failure leaves actionable hint.
- Depends on structured managed-image verification events.
Cross-links: Thread 12 in snapshot (Post-boot bootstrap).

---

