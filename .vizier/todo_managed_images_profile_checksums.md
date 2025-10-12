# Thread 10 — Seamless Alpine Bootstrap
Snapshot: v0.7 (Current)

Goal
- Deliver phase 3–4 of managed images: use QEMU profile injection when a managed image provides kernel/initrd/append/machine, and enrich catalog entries with source checksums/sizes and clearer offline/error copy.

Why (tension)
- Snapshot Thread 10 notes profile hooks exist but are unused; catalog lacks source checksums; offline/error UX can be tightened. This undermines the promise of predictable, verifiable, zero-config bootstrap.

Desired behavior (product level)
- When a managed image’s catalog entry includes a boot profile (kernel/initrd/append/machine), `castra up` launches QEMU with those overrides for the VM(s) using that image.
- Progress events during `up` explicitly note when a profile is applied (e.g., “→ alpine-minimal v2: applied kernel/initrd profile”).
- Catalog entries for managed images include source artifact checksums (and sizes) used to verify downloads before transform; mismatches produce a clear failure with actionable help.
- Offline/error paths provide crisp copy: distinguish no-network vs checksum mismatch vs transform failure; keep exit codes consistent with preflight/IO buckets.

Acceptance criteria
- Given a catalog entry with kernel/initrd/append/machine, launching `castra up` results in QEMU being invoked with the specified overrides; the VM boots successfully, and logs reflect the applied profile.
- Given a corrupted or mismatched download, `castra up` fails before transform with a diagnostic referencing the expected checksum and a suggestion to remove the cached file and retry; exit code falls under IO/launch error class.
- When offline, `castra up` prints an offline-specific message including cache status (hit/miss) and a hint to prefetch; no partial files are left behind.
- Catalog entries include checksum fields for each source artifact; when checks pass, events note “verified source checksums.”

Scope and anchors (non-prescriptive)
- Anchors: src/managed/* (catalog), core/runtime image preparation, ImageManager events; CLI rendering in src/app/up.rs.
- Keep implementation open: QEMU profile wiring and checksum verification strategy may vary; preserve existing manifest and logging locations.

Notes
- Align copy with existing event style; ensure warnings/errors route through diagnostics for consistency.
Snapshot reference bumped to v0.7.1. Clarify that events should include "verified source checksums" prior to transform and errors route through diagnostics with distinct codes/messages for offline vs mismatch.

---

