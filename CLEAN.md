# castra clean command architecture

## Objectives
- Add a first-class `castra clean` subcommand that can purge managed image caches globally and reclaim per-workspace state without manual filesystem surgery.
- Align with the v0.7.2 narrative: sharpen UX (Thread 1), respect lifecycle safety (Thread 2), and complement managed-image trust guarantees (Thread 10) by giving users a supported way to recover from corrupt caches.
- Keep the implementation library-friendly so future GUI/API surfaces can trigger the same cleanup routines without shelling out.

## User experience contract
- Entry point: `castra clean [FLAGS]`. The existing global `--config` flag remains available, so commands still look like `castra --config ./castra.toml clean …`.
- Scope switches:
  - `--global` (or positional `global`) purges caches under the shared projects root (default `~/.castra/projects/**/images`).
  - `--workspace` (default when `--config`, `--state-root`, or discovery succeeds) cleans only the resolved state root.
- Workspace resolution mirrors other commands:
  - `--skip-discovery` keeps the skip+config contract from Thread 1.
  - `--state-root PATH` lets advanced users bypass config parsing (e.g., when the config was deleted but state remains).
- Safety toggles:
  - `--dry-run` lists targets, sizes, and blockers without deleting.
  - `--force` overrides running-process safeguards (never the default).
  - `--include-overlays` opt-in to delete overlay qcow2s; by default overlays are retained even in workspace mode.
  - `--include-logs` and `--include-handshakes` default to true (logs/handshakes are ephemeral), but flags exist in case automation wants to skip them.
  - `--managed-only` suppresses all non-managed artifacts (global mode equivalent).
- Output:
  - Summary table (path, action, bytes reclaimed or reason skipped).
  - Diagnostics via existing reporting channel for warnings (e.g., “broker still running; rerun with `--force` once stopped.”).
  - Events surface through the reporter API so TUI/GUI clients can render progress consistently.
  - Managed image cleanup entries surface verification evidence (image id, root-disk path, total bytes, verification/filesystem delta) sourced from the latest ManagedImageVerificationResult.

## Cleanup scope
- **Managed image cache (default):** `<state_root>/images/**`, `image-manager.log`, manifest files.
- **Ephemeral runtime state (workspace scope):** `logs/`, `handshakes/`, `*.pid`, transient sockets.
- **Optional overlays:** any overlay path declared in `ProjectConfig.vms[].overlay`, respecting whether it lives inside or outside the state root.
- **Global sweep:** iterate through `~/.castra/projects/*`, applying the managed-image plan per directory. Global mode never touches overlays.
- Non-goals: removing user auth material, downloaded ISO/seed files outside Castra, or altering project configs.

## Implementation plan

### CLI layer (`src/cli.rs`)
- Introduce `Commands::Clean(CleanArgs)` with the flags above, mirroring existing doc comments for help output.
- Ensure mutual exclusivity validation in the Clap layer (`--global` vs workspace selectors) for immediate UX feedback.

### App layer (`src/app/clean.rs`)
- New module exporting `handle_clean`.
- Responsibilities:
  - Resolve `CleanScope` using `app::common::config_load_options` when config-driven.
  - Instantiate `core::options::CleanOptions`.
  - Execute `core::operations::clean`, render diagnostics, and print a human-readable summary (reusing status/ports table helpers where practical).
  - Convert emitted events into log lines (e.g., “→ removed 2.3 GiB from …”).
- Register the module in `src/app/mod.rs` and dispatch from `main.rs`.

### Core options & outcomes (`src/core/options.rs`, `src/core/outcome.rs`)
- Add:
  ```rust
  pub struct CleanOptions {
      pub scope: CleanScope,
      pub dry_run: bool,
      pub include_overlays: bool,
      pub include_logs: bool,
      pub include_handshakes: bool,
      pub managed_only: bool,
      pub force: bool,
  }

  pub enum CleanScope {
      Global { projects_root: PathBuf },
      Workspace(ProjectSelector),
  }

  pub enum ProjectSelector {
      Config(ConfigLoadOptions),
      StateRoot(PathBuf),
  }
  ```
- Outcome structs:
  ```rust
  pub struct CleanOutcome {
      pub dry_run: bool,
      pub state_roots: Vec<StateRootCleanup>,
  }

  pub struct StateRootCleanup {
      pub state_root: PathBuf,
      pub project_name: Option<String>,
      pub reclaimed_bytes: u64,
      pub actions: Vec<CleanupAction>,
  }

  pub enum CleanupAction {
      Removed { path: PathBuf, bytes: u64, kind: CleanupKind },
      Skipped { path: PathBuf, reason: SkipReason, kind: CleanupKind },
  }

  pub enum CleanupKind {
      ManagedImages,
      Logs,
      Handshakes,
      Overlay,
      PidFile,
  }
  ```
- Extend `Event` with a `CleanupProgress { path, kind, bytes, dry_run }` variant so reporters can stream updates without string parsing.

### Core cleanup logic (`src/core/operations/clean.rs`)
- Split implementation into a dedicated submodule to keep `mod.rs` readable.
- Workflow:
  1. **Scope resolution**
     - Global: call new helper `project::default_projects_root()` (see below) and gather child directories that look like state roots (contain `images`).
     - Workspace: load project when a config is supplied (reuse `load_project_for_operation`), capturing `ProjectConfig`, `synthetic` flag, and overlays. When only a state root is given, build a lightweight descriptor by inspecting the directory.
  2. **Safety checks**
     - For configs: use `status::collect_status` to verify no VM is “running” unless `--force`.
     - Without config: scan `*.pid` with `runtime::inspect_vm_state`; any live pid blocks cleanup unless forced.
     - Emit diagnostics with remediation (Thread 2 alignment).
  3. **Plan construction**
     - Determine candidate paths for each `CleanupKind` based on inclusion flags.
     - For overlays, respect absolute paths and warn if they escape the state root (user confirmation needed).
     - Compute sizes using `walkdir` or streaming metadata; if a path is missing, record a skipped action with a “not found” reason.
  4. **Execution**
     - If `dry_run`, short-circuit after plan creation, emitting `CleanupAction::Skipped` with `SkipReason::DryRun`.
     - Otherwise, delete directories/files in dependency order (handshakes/logs before removing directories). Use `fs::remove_file` / `remove_dir_all`, capturing IO errors into diagnostics.
     - Sum reclaimed bytes for telemetry and outcome reporting.
     - Emit `Event::CleanupProgress` after each deletion and a `Message` summarizing totals.
  5. **Outcome assembly**
     - Return `OperationOutput::new(CleanOutcome { … }).with_diagnostics(diagnostics).with_events(events)`.
- Re-export `clean` from `core::operations::mod.rs`.

### Shared utilities
- `src/core/project.rs`: expose a new `default_projects_root()` helper that mirrors `config::default_state_root` but returns `~/.castra/projects`. Internally reuse `config::user_home_dir` (promote it to `pub(crate)`).
- `src/core/runtime.rs`: add `fn running_processes(state_root: &Path) -> Vec<RunningProcess>` to share the PID scanning logic with cleanup, avoiding ad-hoc parsing.
- Consider a `StateRootInspector` struct in the new cleanup module that can hydrate overlay paths from `ProjectConfig` or fallback globs (`*.qcow2`, `overlays/**`) when config is absent, flagging uncertain entries via diagnostics.

## Safety & diagnostics
- Never delete while `status::collect_status` reports a running VM or broker unless `--force`.
- Provide explicit guidance in diagnostics (“Use `castra down` or rerun with `--force` if you are sure nothing is running.”).
- Handle permission errors gracefully, especially in global mode where stale directories might be owned by root (see existing Thread 10 cache guidance).
- For overlays outside the state root, require `--include-overlays` and log the absolute path so users understand the blast radius.

## Testing strategy
- Unit tests for CLI parsing (mirroring `cli.rs` tests).
- Core cleanup tests using `tempfile`:
  - Managed image cache deletion (create fake manifest/sha file, assert removal and byte accounting).
  - Dry-run mode verifies no deletions occur.
  - Running-process guard: create a fake pidfile with an alive helper process, ensure cleanup refuses unless forced.
  - Overlay opt-in: ensure overlays survive without the flag and are removed when requested.
- Integration smoke test in `tests/clean.rs` invoking `clean` through the public API with a temporary project config.
- Regression placeholder tying into Thread 10: simulate checksum mismatch flow that references the new command in error copy.

## Documentation & follow-up
- Update `README.md` (CLI section) and author a short `docs/housekeeping.md` entry describing cache locations and examples (`castra clean --global`, `castra clean --workspace --config ./castra.toml --dry-run`).
- Cross-reference from managed-image checksum mismatch errors (“Run `castra clean --workspace … --managed-only` to remove cached bits.”).
- Ensure the .vizier TODO for Thread 10 references the new command as the supported cache purge path; adjust acceptance wording if necessary.

## Interactions with active threads
- **Thread 1 (skip-discovery contract):** cleanup obeys the same `--skip-discovery` guard, preventing accidental global nukes when users expect CLI auto-discovery to run.
- **Thread 2 (lifecycle):** running-process detection leverages the same inspection helpers, keeping shutdown semantics centralized and consistent.
- **Thread 3 (broker reachability):** cleanup clears stale handshakes and broker pidfiles, which helps maintain accurate freshness metrics once Thread 3 lands.
- **Thread 6 (ports active view):** no direct changes, but removing stale pidfiles prevents ports data from misreporting running VMs after a clean.
- **Thread 10 (managed images):** official purge path complements checksum enforcement, giving docs and diagnostics a clear remediation command.
