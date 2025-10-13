Snapshot v0.7.4 update — Major pieces shipped in commit 349c47d and 1583c8c

Status
- Alpine boot profile with kernel/initrd and source checksums is implemented; verification occurs at fetch time, and partial/corrupt files are cleaned with actionable messages. Events/log copy includes applied boot profile and verified checksums. Offline cache diagnostics improved.

Remaining scope (product-level)
- Ensure machine-parseable Events exist for both “verified checksums” and “applied boot profile” with stable fields consumable by external tooling; expand test coverage to assert event emission.
- Document cache layout and remediation path in docs, referencing `castra clean` once available.
- Byte/accounting surfaced in status or logs when cache issues are detected (optional, aligns with CLEAN acceptance).

Acceptance updates
- During `up`, an Info-level event with structured fields is emitted upon checksum verification and upon applying a boot profile; tests assert presence and field stability.
- README/docs include a short section on managed profiles and cache verification, with failure modes and suggested remedies.

---

