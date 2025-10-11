# Seamless Alpine Bootstrap v1 (Thread 10 — Snapshot v0.6)

Narrative linkage
- Advances Thread 10: Seamless Alpine Bootstrap (see Snapshot v0.6). Resolves the promise that `castra up` from an empty directory boots a managed Alpine VM without user configuration, while preserving existing BYO image paths.

Product goals
- Zero-config path: `castra up` in an empty directory results in a running Alpine x86_64 VM using a managed base image.
- Catalog-driven assets: Alpine artifacts are downloaded, validated, transformed, cached, and reused.
- UX: Clear progress/caching messages in CLI; detailed image-manager logs under .castra/logs; failure modes are specific (download, checksum, tool missing).
- Backward compatibility: Existing castra.toml flows (path-based base images) remain unchanged.

Acceptance criteria
1) Empty-dir boot
   - Given an empty directory, when I run `castra up`, then:
     • A synthesized project is used (no castra.toml is written).
     • Assets are stored under `.castra/images/alpine-minimal/v1/` and a manifest is recorded.
     • First run shows download/progress; subsequent runs show cache hits.
     • A VM boots successfully with Alpine defaults; QEMU logs/serial logs appear under .castra/logs.
2) Config opt-in
   - Given a castra.toml that specifies `managed_image = { name = "alpine-minimal", version = "v1" }`, then `castra up` uses the managed base image and kernel args from the catalog.
3) BYO QCOW unaffected
   - Given a castra.toml with a `base_image = ".../file.qcow2"`, then the behavior matches current releases; no downloads occur.
4) Error clarity
   - If network is unavailable or checksum fails, CLI prints a specific, actionable error; no partial files are left behind (only `*.partial` may exist). Missing `qemu-img` produces a clear preflight error.
5) Status visibility
   - `castra status` in a project that used a managed image indicates VM state as today; image-manager specifics are available in logs.

Scopes of work (product-level)
A) Config Resolution Layer
- Add a default-project branch when discovery fails: `load_or_default_project` returns an in-memory ProjectConfig referencing the Alpine managed image and a per-project overlay path under `.castra/`.

B) Managed Image Catalog
- Introduce a `managed` module with a static catalog entry `alpine-minimal@v1` capturing artifact URLs, checksums/sizes, and a QEMU profile (kernel, args, disk hints). No runtime catalog updates in v1.

C) ImageManager Pipeline
- Provide `ensure_image(spec, runtime)` to materialize assets under `.castra/images/<id>/<ver>/`, with:
  • Manifest tracking (hashes, timestamps, catalog version hash).
  • Download with resume and progress reporting; partial files end with `.partial` and are atomically renamed post-verify.
  • Transformations needed for Alpine: StripVhdFooter; qemu-img convert raw→qcow2; rename to stable final names. Hashes verified after download and as applicable post-transform.
  • Early failure on missing `qemu-img` when required.

D) Project Configuration Model
- Extend VM definition to accept `managed_image` alongside the existing `base_image` path. Internally, resolve `BaseImageSource::{Path|Managed}` and plumb the resolved base image path for downstream overlay creation unchanged.

E) Runtime Integration
- Thread the ImageManager via runtime context; ensure manager logs under `.castra/logs/image-manager.log`. Resolve managed paths before overlay creation to avoid mismatched overlays.

F) QEMU Launch Adjustments
- Allow QEMU args to be augmented by the catalog's QemuProfile: `-kernel`, `-append`, optional `-initrd`, and device hints aligned with the managed disk. Respect user opt-out if explicitly configured.

G) CLI Feedback & Logging
- Print high-level progress lines (download start/complete, cache hits). Keep copy style consistent with existing warnings/status. Detailed logs go to `.castra/logs/image-manager.log`.

H) Tests & CI fixtures
- Integration: `up` from empty dir boots Alpine (use a pre-seeded cache fixture to avoid live downloads). Regression: BYO configs unchanged. Unit: transformation primitives and manifest reconciliation validate.

Rollout plan
- Phase 1: Catalog + config model + ImageManager skeleton (behind feature flag if needed).
- Phase 2: Wire fallback project and manager into `up` (and `status` as needed).
- Phase 3: QEMU profile injection (kernel boot path).
- Phase 4: CLI copy polish, docs/help updates, fixtures in CI.

Out-of-scope (v1)
- Multiple managed images/architectures; remote catalog updates; complex compression handling; shared overlays across projects.

Notes
- This TODO aligns with existing threads 4 (Config/discovery), 5 (Images/storage), and 7 (Observability). It is the primary driver for zero-config onboarding.
