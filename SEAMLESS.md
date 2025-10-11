# Seamless Alpine Bootstrap Plan

This document captures the intended architecture for bringing the “zero‑configuration” Alpine VM experience into Castra. It focuses on how to hydrate a managed Alpine image, how that image integrates with the existing QEMU adapter, and how we surface the experience to users in a seamless way.

---

## Objectives
- Boot a working VM with `castra up` even when there is no `castra.toml` on disk.
- Ship a built-in Alpine Linux image definition that we can fetch, validate, transform, and reuse as the project base image.
- Keep the existing “bring your own QCOW2” path fully compatible.
- Minimize one-off logic in command handlers by centralizing image discovery, download, and transformation pipelines.
- Provide clear user feedback (progress, errors, caching) without introducing configuration churn.

Non-goals for the first cut:
- Supporting multiple built-in images or architectures beyond the predefined Alpine x86_64 guest.
- Building dynamic catalog updates at runtime (the catalog lives in source for now).
- Solving kernel/initrd/boot parameter discovery beyond what the Alpine profile requires.

---

## High-Level UX Flow
1. User runs any Castra command (e.g., `castra up`) from an empty directory.
2. Castra attempts config discovery; none is found.
3. We transparently fall back to a synthesized project that declares the Alpine managed VM.
4. Prior to launch, an *image manager* ensures all required Alpine artifacts exist under `.castra/images/alpine-minimal/v1/`.
5. Assets are downloaded, verified, transformed (strip VHD footer, convert to QCOW2, etc.), and recorded in a manifest.
6. The VM overlay is created on top of the managed QCOW base (unless the user supplied their own).
7. QEMU launches using the managed disk and kernel arguments defined by the profile.
8. Subsequent runs reuse the cached artifacts and skip network work unless the manifest is missing/stale.

Users who *do* provide `castra.toml` still get the current behavior, but they may opt into the built-in image by referencing it from configuration.

---

## Architectural Components

### 1. Config Resolution Layer
- `resolve_config_path` keeps its current role but gains a “default project” branch when discovery fails.
- A new helper (e.g., `load_or_default_project`) tries `load_project_config`; on failure it synthesizes an in-memory `ProjectConfig` from a bundled spec.
- The synthesized project references the managed image via a new config abstraction (detailed below) instead of hard-coded file paths.
- Commands that previously bubbled up `ConfigDiscoveryFailed` now receive a usable `ProjectConfig` and proceed as normal.

### 2. Managed Image Catalog
- Introduce a `managed` module (e.g., `src/managed/mod.rs`) housing:
  - A static catalog: `ManagedImageSpec { id, version, artifacts, qemu, customization }`.
  - Each `ManagedArtifactSpec` holds source URL, expected checksum/size, final filename, and a transformation pipeline.
  - QEMU metadata (`QemuProfile`) captures kernel/initrd paths, machine type overrides, kernel command line, and disk configuration hints.
- The Alpine profile is encoded in this catalog using the constants already proposed:
  - Kernel URL: `vmlinuz-virt`
  - Rootfs URL: `aws_alpine-3.22.2-x86_64-bios-tiny-r0.vhd`
  - Transformations: `strip_vhd_footer` ⇒ `qemu-img convert -f raw -O qcow2` ⇒ rename to `rootfs.ext4`
- Future profiles can reuse the same structure.

### 3. Asset Acquisition Pipeline
- Add an `ImageManager` type responsible for ensuring assets exist under the Castra state root.
- Core responsibilities:
  - Resolve storage layout: `.castra/images/<image id>/<version>/<artifact>`.
  - Track a manifest (`manifest.json` or `.stamp`) containing checksums, creation timestamps, and the catalog hash/version to detect drift.
  - Fetch artifacts with resume support and progress reporting (initially we can use blocking `reqwest` or `ureq` until async refactor lands).
  - Execute transformation steps defined in the spec. Supported primitives for the Alpine pipeline:
    - `StripVhdFooter` (strip 512-byte VHD footer in-place; operation implemented in Rust).
    - `QemuImgConvert { input_format, output_format, output }` (requires `qemu-img`; we fail early if unavailable).
    - `Rename`.
    - `Decompress` (placeholder for future tarballs).
  - Validate SHA-256 (or stronger) hashes after download and after transformations when applicable.
- Expose `ensure_image(&ManagedImageSpec, &RuntimeContext) -> ManagedImagePaths` returning resolved kernel/rootfs paths for use downstream.

### 4. Project Configuration Model
- Extend `VmDefinition`/`RawVm` to support “managed base images” without breaking existing path-based configuration.
  - Introduce `BaseImageSource` enum:
    ```rust
    pub enum BaseImageSource {
        Path(PathBuf),
        Managed { id: String, version: String, artifact: ManagedDiskKind },
    }
    ```
  - `ManagedDiskKind` differentiates between the primary disk QCOW and other potential disks.
  - `VmDefinition` stores both the selected source **and** the resolved `PathBuf` once the image manager runs.
- Update config parsing rules:
  - Allow `[[vms]]` entries to specify either `base_image = ".../file.qcow2"` *or* `managed_image = { name = "alpine-minimal", version = "v1" }`.
  - Default overlay path remains mandatory; the manager will derive the QCOW base location and feed `ensure_vm_assets`.
- For the synthesized project, populate `VmDefinition` with `BaseImageSource::Managed` and a default overlay path (e.g., `.castra/alpine-minimal-overlay.qcow2`).

### 5. Integration With Runtime Context
- `prepare_runtime_context` continues to compute `state_root` and `log_root`.
- It gains a reference to `ImageManager` so the manager inherits path and logging conventions (e.g., `.castra/logs/download.log`).
- Store resolved managed image paths in a new `ResolvedVmArtifacts` structure cached per VM for use by downstream steps.
- Ensure we surface dependency errors *before* overlay creation to prevent mismatched overlays.

### 6. `ensure_vm_assets` Enhancements
- Recognize managed sources:
  - If `BaseImageSource::Managed`, call `image_manager.ensure_image(spec)`; update the VM’s `base_image` path to point at the managed QCOW produced by the pipeline.
  - Re-run overlay checks using the resolved path (current logic remains valid).
- Preserve the existing behavior for path-based VMs.
- Improve user messaging:
  - On first run, print status like `→ alpine-minimal v1: downloading rootfs (45 MB / 60 MB)…`.
  - Distinguish between download failures, checksum mismatches, and missing `qemu-img`.

### 7. QEMU Launch Path Adjustments
- Leverage the `QemuProfile` metadata that the managed catalog exposes:
  - `-kernel <path>` pointing at `vmlinux.bin`.
  - `-append <args>` with Alpine-friendly defaults (`root=/dev/vda1 console=ttyS0` etc.).
  - Optional `-initrd` if any future image needs it.
- Add hooks in `launch_vm` to accept profile-supplied device args (e.g., ensuring virtio block or network devices align with the managed disk format).
- Keep user-defined overrides possible by letting `VmDefinition` opt out (e.g., custom configs can set `boot_profile = "native"` to skip injecting kernel args).

### 8. CLI Feedback & Logging
- Emit high-level progress in the CLI:
  - Download start/completion lines.
  - Cache hits: `→ alpine-minimal v1: cache hit (skipping download)`.
- Record detailed logs (failures, manifest writes) inside `.castra/logs/image-manager.log`.
- When downloads are skipped due to offline mode or missing network, fail fast with a message explaining why the default VM cannot boot.

### 9. Testing Strategy
- Add integration coverage:
  - `castra up` in an empty temp dir boots the managed VM (mock or fixture network—tests can inject a pre-populated cache to avoid live downloads).
  - Regression tests to ensure existing configs remain valid.
  - Unit tests for transformation primitives (especially `strip_vhd_footer` and manifest reconciliation).
- Provide a hermetic way to seed the cache during CI (e.g., copy fixture artifacts into the expected `.castra/images/...` directory).

### 10. Migration & Rollout
- Stage the refactor in phases:
  1. Introduce catalog, config model changes, and manager skeleton (behind a feature flag if needed).
  2. Wire fallback project and managed image resolution into `castra up`.
  3. Add QEMU profile integration and direct kernel boot support.
  4. Update CLI messaging + docs (`README`, help text).
- Existing commands keep their behavior throughout; new paths are additive.

### 11. Open Questions / Follow-ups
- Should managed image metadata be versioned separately from Castra releases (e.g., remote manifest fetch)? For now we encode it statically.
- How do we handle partial downloads if users interrupt the process? Proposed answer: write to `*.partial` and atomically rename after checksum passes.
- Will we need compression handling (e.g., `.gz`, `.xz`) immediately? Alpine URLs currently provide raw files; we can stub decompression steps for future use.
- Should overlays be managed per project or per image? Current plan keeps overlays per project to avoid sharing state between workspaces.
- Does the refactor require async networking? If the wider refactor migrates to async soon, design the manager API to be “async-ready” (trait or feature).

---

## Next Steps
1. Sketch the new `managed` module scaffolding and adjust `ProjectConfig` parsing for `managed_image`.
2. Implement `ImageManager` including cache layout, manifest handling, and transformation primitives.
3. Integrate manager invocation into command flows (`handle_up`, later `handle_status` for status visibility).
4. Add the QEMU profile injection.
5. Polish UX strings, add docs/tests, and validate on both macOS (HVF) and Linux (KVM).

This plan should provide enough detail to proceed with implementation, while leaving room for iteration as the broader refactor lands.

