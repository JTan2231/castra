
---
Edit (align with stamp-free bootstrap)

- Remove mention of stamp semantics controlling NoOp; clarify that persistence of stamps is not part of stateless runs. Durable host evidence is logs/events only.
- CLEAN integration remains: reclaimed bytes from ephemeral layers are reported; managed-image evidence linkage is optional.
- Add acceptance that persistent mode must also bypass ephemeral cleanup and that UI clearly signals when persistence is enabled.

Acceptance delta
- Reruns behave identically regardless of prior runs; guest filesystem starts pristine unless persistent mode was used previously for that VM and left durable disks by explicit choice (out-of-scope for default path).

Cross-links
- Thread 12 (bootstrap) now stamp-free; ensure narratives match in docs.
---

---

