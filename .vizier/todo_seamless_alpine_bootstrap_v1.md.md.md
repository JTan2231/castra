---
Update (SNAPSHOT v0.7)

Evidence
- load_or_default_project now synthesizes a default project when discovery fails, referencing `managed_image = { name = "alpine-minimal", version = "v1" }` with a per-project overlay.
- managed module added with a static catalog and ImageManager. ensure_image() downloads the Alpine VHD, resumes partials, converts via qemu-img (vpc→qcow2), records a manifest, emits CLI events, and logs to `.castra/logs/image-manager.log`.
- up integrates ImageManager before overlay creation; prints progress/cache-hit lines; launches VM with user-mode NAT; status/logs behave as documented.

Gaps
- No kernel/initrd injection for Alpine v1 (profile scaffolding exists but unused).
- Catalog lacks source checksums/size for Alpine artifact; source verification is skipped.
- Offline/download-failure messaging can be more explicit.

Acceptance alignment
- 1) Empty-dir boot: ACHIEVED (assets under `.castra/images/alpine-minimal/v1`, manifest written; events shown; subsequent runs cache-hit). Kernel-less boot path acceptable for Alpine v1.
- 2) Config opt-in: ACHIEVED (managed_image parsing and resolution work).
- 3) BYO QCOW unaffected: ACHIEVED (path-based base_image path preserved).
- 4) Error clarity: PARTIAL (final checksum recorded; source checksum/size checks pending; offline copy to tighten).
- 5) Status visibility: ACHIEVED (no special image-manager status; details in logs).

Next steps (Phase 3–4)
- Populate checksum/size in catalog and enforce source verification with clear failure copy.
- Add kernel/initrd profile support for images that require it; maintain opt-out for user configs.
- Improve offline handling: detect lack of network early and surface a crisp message for the zero-config path.
- Copy polish: unify event strings with existing CLI style; add brief summary after image acquisition.


---

