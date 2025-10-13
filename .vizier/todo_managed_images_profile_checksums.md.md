Progress note — Snapshot v0.7.6

- Partial delivery: Alpine boot profile wiring and checksum/size verification shipped with clearer diagnostics and success copy. Machine-parseable Events and docs still pending.
- Evidence: snapshot entries; code paths in src/managed/mod.rs and reporter/logs show improved messages.

Next steps
- Emit structured Events: `ChecksumsVerified { image, artifacts }` and `BootProfileApplied { image, profile }` (names illustrative) and surface via reporter API.
- Document cache layout, remediation flow (e.g., clean managed cache) and cross-link CLEAN.md.
- Add tests asserting event emission on both success and mismatch paths.


---

