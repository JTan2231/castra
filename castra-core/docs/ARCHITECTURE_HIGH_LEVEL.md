# Castra Architecture (High-Level)

Castra is a CLI-forward orchestration harness that wraps QEMU-based guests with reproducible state roots, a cached base image pipeline, and a structured observability surface. This document walks the repository from the user experience down to the runtime that manipulates QEMU processes and the direct SSH session helpers exposed by the harness/UI.

## System Overview

At a high level, Castra layers the following subsystems:

- **UX & Entry Points** – Clap-driven CLI (`src/main.rs`, `src/cli.rs`) plus a feature-gated library surface (`src/lib.rs`) for embedding.
- **Application Adapters** – Command-specific handlers under `src/app/` translate CLI arguments into typed core options, invoke operations, and render events/diagnostics for humans.
- **Core Operations API** – `src/core/` exposes pure library functions (init/up/down/status/etc.) that accept typed `Options`, emit structured `Event`s via a `Reporter`, accumulate `Diagnostic`s, and return typed `Outcome`s.
- **Configuration & Project Model** – `src/config.rs` and `src/core/project.rs` discover, parse, validate, and synthesize `ProjectConfig` structures that describe VMs, lifecycle policy, and bootstrap behaviour.
- **Runtime Layer** – `src/core/runtime.rs` resolves base image paths (downloading the default Alpine qcow2 on demand), prepares overlays, ensures host headroom, and spawns/tears down QEMU processes with cooperative shutdown semantics.
  The runtime is parameterised by `VmLaunchMode`, allowing callers to choose the legacy `-daemonize` launch or an attached spawn where QEMU remains a child of the embedding process. Attached runs rely on the runtime to synthesise pidfiles from the spawned child when QEMU is slow to flush them, ensuring shutdown continues to function via the existing pidfile path.
- **Bootstrap & Post-Boot Automation** – `src/core/bootstrap.rs` runs per-VM bootstrap pipelines (e.g. Nix flakes) while Vizier health signals confirm connectivity via the harness.
- **Observability & Maintenance** – Event stream definitions (`src/core/events.rs`), diagnostics (`src/core/diagnostics.rs`), reporter plumbing, status/log collectors, and the clean workflow tie the lifecycle together.

The sections below dive into each layer and call out the principal modules, data flows, and responsibilities.

## UX & Entry Points

- `src/main.rs` boots Clap's `Cli`, ensures a subcommand is present, and dispatches to `src/app` handlers. Errors are normalized to `ExitCode`s via `app::error::exit_code`.
- `src/cli.rs` defines the CLI contract: top-level flags (e.g. `--config`) and subcommand enums. It also performs light parsing such as bootstrap override parsing into `BootstrapOverrideArg`.
- `src/lib.rs` re-exports the `core` module plus config/error APIs for embedding. The crate can be built without the CLI feature, allowing Castra to be consumed as a library.
- `castra-ui` consumes the library surface with `VmLaunchMode::Attached`, caches the `UpOutcome` state/log roots, and registers shutdown hooks (window close, Quit, process signals) that run `operations::down` before exiting so orphaned QEMU processes are cleaned up automatically.

## Application Layer (`src/app`)

Each command has a dedicated module (`up.rs`, `down.rs`, `status.rs`, `clean.rs`, etc.) that does three things:

1. **Translate CLI arguments to core options** – via helpers in `app/common.rs` and typed structs from `src/core/options.rs`.
2. **Invoke core operations** – using `castra::core::operations::{up, down, clean, ...}`.
3. **Render results** – convert structured `Event`s and `Diagnostic`s into human-friendly stdout/stderr output. For example, `app/down.rs` renders the ordered shutdown lifecycle (`ShutdownRequested → CooperativeAttempted → …`) that the runtime now guarantees (Thread 2 in `.vizier/.snapshot`).

The application layer intentionally stays I/O-bound and side-effect free beyond formatting; the heavy lifting happens inside `src/core`.

## Core Operations API (`src/core`)

The `core` module is the public programmatic interface. Key building blocks:

- **Options (`src/core/options.rs`)** – strongly typed option structs per command (e.g. `UpOptions`, `DownOptions`, `CleanOptions`). They carry config discovery hints, override knobs, and bootstrap overrides in a normalized form.
- **Diagnostics (`src/core/diagnostics.rs`)** – severity-tagged messages with optional path/help metadata. Diagnostics travel alongside outcomes without aborting the workflow.
- **Events (`src/core/events.rs`)** – structured telemetry covering lifecycle, cached image downloads, bootstrap steps, cleanup progress, etc. These drive both CLI rendering and machine consumption (JSON).
- **Reporter (`src/core/reporter.rs`)** – minimal trait that callers implement to observe emitted events; the CLI uses an adapter that buffers events while keeping streaming semantics.
- **Outcomes (`src/core/outcome.rs`)** – typed results for each command (e.g. `UpOutcome`, `DownOutcome`, `CleanOutcome`) so downstream tooling can inspect state without parsing text.
- **Operations (`src/core/operations/`)** – orchestrators for each command. `mod.rs` stitches together configuration loading, runtime preparation, Vizier launch, bootstrap runs, shutdown, port summaries, log collection, and cleaning. Each operation returns an `OperationOutput<T>` bundling the outcome, diagnostics, and events.

### Configuration & Project Model

- `src/config.rs` defines the canonical config schema (`ProjectConfig`, `VmDefinition`, `LifecycleConfig`, `BootstrapMode`, etc.) and provides parsing, validation, and helper defaults (timeouts, base image/overlay derivation).
- `src/core/project.rs` resolves the effective config via `ConfigLoadOptions`. It supports discovery up the directory tree, explicit paths, and synthetic defaults (`synthesize_default_project`) when running in library contexts. The module also surfaces helper utilities such as `config_state_root` (where per-project state is stored) and `default_config_contents` for `castra init`.
- Configuration warnings are converted into diagnostics so the CLI can separate "config health" messages from operational warnings.

### Base Image & Asset Preparation

- `core/config` resolves a `BaseImageSource` per VM, marking provenance (`DefaultAlpine` vs `Explicit`).
- `core/runtime::ensure_vm_assets` ensures the base image exists (downloading/verifying the default Alpine qcow2 when necessary), prepares overlays, and streams download status via `Event::Message`.
- `core/outcome::VmLaunchOutcome` records the resolved base image path and provenance so automation can audit which qcow backed each VM.

### Runtime & Host Integration

The runtime (`src/core/runtime.rs`) bridges higher-level operations to actual host processes:

- **Context preparation** – `prepare_runtime_context` creates the state root (logs, images), locates QEMU binaries, and chooses accelerators.
- **Preflight checks** – `check_host_capacity`, `check_disk_space`, and `ensure_ports_available` enforce headroom and exclusive port usage before launch.
- **Session metadata** – bootstrap events surface resolved SSH details (user, host, port, identity options). The harness/UI reuse that metadata to drive direct agent sessions; no in-guest steward or external shim is required.
- **VM launch** – `launch_vm` builds the QEMU command (daemonized, virtio devices, serial log, QMP socket on Unix) and records pidfiles/logs. It emits `Event::VmLaunched` when successful.
- **Cooperative shutdown** – `shutdown_vm` enforces the event ordering captured in `.vizier/.snapshot`: it attempts QMP ACPI (or marks channel unavailable), tracks deadlines (`ShutdownTimeouts`), escalates via SIGTERM/SIGKILL as needed, and emits `ShutdownComplete(outcome, total_ms, changed)` with granular reasons (`CooperativeTimeoutReason`).
- **State inspection** – helpers like `inspect_vm_state` power `status`, `ports`, and `clean` by reading pidfiles and checking process liveness. Legacy broker pidfiles are ignored and pruned.

Unix-specific QMP interactions handle cooperative ACPI powerdown; on non-Unix platforms Castra documents that cooperation is unavailable and proceeds directly to signal escalation.

### Bootstrap Pipeline

- `src/core/bootstrap.rs` executes per-VM bootstrap plans after guests prove connectivity via Vizier health signals. It runs workers in parallel (scoped threads), streams bootstrap events over an `mpsc::channel`, and records durable logs under the state root—without persisting host-side stamps.
- Bootstrap respects `BootstrapMode` (skip/auto/always), gating on Vizier readiness. Outcomes (`BootstrapRunOutcome`) distinguish `Success`, `NoOp`, and `Skipped`, and diagnostics explain missing plans or failed steps.
- Thread 12 progress (see `.vizier/.snapshot`) is reflected here: overrides (global/per-VM) are applied before launch (`apply_bootstrap_overrides`), reruns always attempt the pipeline, and failure modes surface as structured events (`BootstrapFailed`) with durable error logs.

### Status, Logs, and Diagnostics

- `src/core/status.rs` synthesizes a `StatusSnapshot` with per-VM rows (`VmStatusRow`) summarizing VM state, uptime, and declared forwards. It consumes pidfiles and reported bootstrap metadata; Vizier telemetry flows through the harness.
- `src/core/logs.rs` aggregates per-VM logs, returning `LogSection`s and optional `LogFollower`s for tail -f semantics (`castra logs --follow`).
- Events defined in `src/core/events.rs` ensure all long-running operations stream machine-readable telemetry (cached image downloads, bootstrap steps, cooperative shutdown, cleanup progress). This underpins both CLI rendering and automation that consumes JSON.
- Diagnostics accompany every operation and are rendered in `app/common.rs` (with helpers to group config warnings separately).

### Cleaning & State Maintenance

- `src/core/operations/clean.rs` removes cached images, overlays, logs, and pidfiles. It supports dry-run mode, workspace-only cleaning (current project), or global pruning under `~/.castra/projects`.
- The cleaner coordinates with runtime helpers to avoid disrupting live VMs, records reclaimed bytes, and emits `Event::CleanupProgress` entries so automation can reconcile storage changes.

## Command Flows

### `castra up`

1. CLI parses flags (`--force`, `--bootstrap`, etc.) into `UpArgs`.
2. `app::up::handle_up` builds `UpOptions` and calls `core::operations::up`.
3. The operation loads/validates the project (`load_project_for_operation`), applies bootstrap overrides, and runs status preflight (`status_core::collect_status`) to ensure no guests are already running.
4. Runtime preflights host capacity, disk headroom, and port conflicts.
5. For each VM, `ensure_vm_assets` makes sure the base image is ready (downloading the default Alpine qcow2 if needed), provisions overlays, and emits events for overlays/download progress.
6. Each VM is started (`launch_vm`), streaming `VmLaunched` events, and Vizier is launched via the configured `VizierLauncher`.
7. Bootstrap workers execute as needed, publishing structured status events and capturing diagnostics.
8. The aggregated `UpOutcome` reports launched VMs, bootstrap summaries, diagnostics, and the event log for rendering.

### `castra down`

1. CLI maps overrides (`--graceful-wait-secs`, `--sigterm-wait-secs`, etc.) to `DownOptions`.
2. `core::operations::down` loads the project, materializes shutdown timeouts, and spins per-VM threads to run `runtime::shutdown_vm` concurrently.
3. Events stream in shutdown order, giving the CLI enough information to warn about forced terminations or unavailable cooperative channels.
4. After VMs stop, the outcome lists per-VM results so the CLI can warn about forced shutdowns (Thread 2 acceptance criteria).

### `castra status`

- Loads the project, inspects VM pidfiles, and returns a snapshot. CLI renders tabular summaries and can emit JSON via the library interface; Vizier health is reported through the harness stream.

### `castra clean`

- Resolves the state roots to inspect (current config or global projects root), introspects running processes to avoid unsafe deletion, and removes caches/logs/overlays. Diagnostics explain skipped roots, and events summarize reclaimed bytes.

### `castra bus` / `castra broker`

### `castra bus` / `castra broker`

- These commands have been removed. The harness owns session metadata and event publication; direct SSH helpers replace the vizier tunnel workflow described in legacy docs.

## Cross-Cutting Concerns

- **State Layout** – Each project keeps a state root (default `.castra/<project>/`) containing `logs/`, `images/`, `bootstrap/`, and pidfiles. Legacy broker handshake/bus logs are pruned when encountered; Vizier telemetry supersedes them.
- **Concurrency** – Operations favor scoped threads (`std::thread::scope`) and channels for parallel VM work (bootstrap runs, shutdowns) while streaming events to the reporter.
- **Error Handling** – `src/error.rs` centralizes failure types; operations propagate `Error` while still returning accumulated diagnostics/events for partial progress.
- **Testing** – Rust unit tests exist where parsing/logic is localized (e.g. CLI bootstrap override parsing). Broader behaviours rely on integration/smoke tests (not in-tree here) and instrumentation described in `.vizier/.snapshot` threads.
- **Docs & Roadmap** – `.vizier/.snapshot` highlights active threads (cooperative shutdown lifecycle, bootstrap pipeline, stateless overlay messaging). Canonical TODO files under `.vizier/` provide implementation breadcrumbs that align with the architecture above.

## Extensibility Notes

- Adding a new CLI command typically means creating a handler in `src/app`, a typed options/outcome struct in `src/core`, and wiring the behaviour into `src/core/operations`.
- Integrations can consume Castra as a library by disabling the default CLI feature and calling `castra::core::operations::*` with custom reporters.
- Enhancements to base image caching (alternate defaults, mirror selection) live in `src/core/runtime` / `src/config` so they stay aligned with overlay preparation.

## QEMU & System Dependencies

- Castra expects `qemu-system-*` on `PATH` (auto-detected across x86/aarch64) and optionally `qemu-img` for overlay creation. Missing binaries surface as `PreflightFailed` errors during runtime context preparation.
- Cooperative shutdown currently relies on QMP sockets (Unix); non-Unix platforms fall back to signal-only shutdown paths with explicit `CooperativeMethod::Unavailable` events.

By following the layers above, contributors can trace any CLI command from user input through configuration, runtime orchestration, base image caching, and down to the QEMU process tree, while understanding where observability and cleanup hook into the lifecycle.
