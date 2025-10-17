
---
Update (align with stamp-free bootstrap policy)
- Remove any reference to host-side bootstrap stamps. Durable artifacts are limited to logs/events; rerun behavior is independent of guest disk state and host stamps.

Clarifications
- UX: After teardown, render a concise notice per VM in TTY mode (e.g., "vmA: ephemeral changes discarded; see CLEAN for reclaimed bytes"). JSON output remains unchanged.
- CLEAN: reclaimed-bytes reporting should attribute to ephemeral layers and appear even when managed-image evidence is unavailable.
- Validation: Any CLI/config suggesting persistence must be rejected with a clear message that points to exporting via SSH before shutdown.

Acceptance addendum
- No stamp files are created or consulted as part of ephemeral storage handling.
---

---

