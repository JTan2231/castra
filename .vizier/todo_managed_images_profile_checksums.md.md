
---
Evidence update (2025-10-12):
- The only catalog entry (alpine-minimal@v1) ships without kernel/initrd boot data and lacks source checksums (src/managed/mod.rs:705, :720). Boot override plumbing in launch flow is present but unused. This keeps Thread 10 unresolved until a new catalog revision lands with a boot profile and checksums.

Acceptance refinement:
- Catalog entry includes kernel, initrd, cmdline, and machine profile when applicable, plus SHA256 sizes for all source artifacts. Offline errors must reference the missing checksum context.


---

