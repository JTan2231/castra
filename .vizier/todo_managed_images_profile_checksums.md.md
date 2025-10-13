Status update — delivery and remaining work (Snapshot v0.7.5)
- Delivery: Boot profile (Alpine) wiring and checksum/size verification landed; diagnostics improved and success copy emitted.
- Remaining: emit machine-parseable Events for verification results and applied profile; document cache layout and remediation steps; align with `clean` reclaimed-bytes accounting.

Acceptance refinement
- OperationOutput and logs must include structured Events for: `ImageVerified{profile, source, checksum}` and `BootProfileApplied{profile, kernel, initrd}`.
- Docs: add a section to CLEAN.md linking which caches and images are prunable and how verification interacts with `clean` dry-run/force modes.

---

