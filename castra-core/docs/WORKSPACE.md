# Castra Workspaces

Castra keeps every project’s ephemeral runtime state inside a workspace rooted at a `.castra` directory. The workspace holds cached base images, bootstrap staging areas, session metadata, and process bookkeeping so that `castra up`, `status`, `down`, and `clean` can coordinate without leaking state across runs. This note documents how the workspace is picked, how `.castra`-prefixed paths resolve, and what lives under the directory during each phase of the lifecycle.

When multiple workspaces are active, `castra status`, `castra ports`, and `castra down` (without `--config`) aggregate across all of them using the metadata recorded under `metadata/workspace.json`. Use `--workspace <id>` to target a single entry; IDs correspond to the `workspace.id` field captured in the metadata and rendered in the CLI headers.

## Workspace Selection
- **Default naming.** When a config omits `[project].state_dir`, Castra hashes the configuration’s parent directory and combines it with a slugified project name: `~/.castra/projects/<slug>-<hash>`. (`default_state_root` in `src/config.rs`.)
- **Global projects root.** The shared prefix `~/.castra/projects` (`default_projects_root` in `src/core/project.rs`) lets multiple checkouts of the same project keep isolated state roots while still allowing global maintenance via `castra clean --global`.
- **Synthetic configs.** Auto-synthesized configs (when discovery fails and `--allow-synthetic` is set) receive the same slug-and-hash workspace under the global projects root.

## Overriding the Location
- `[project].state_dir` accepts an absolute path or a path relative to the config file. Teams often point this at `.castra/` or `.castra/state` inside the repo to keep all artifacts co-located (`resolve_path` usage in `load_project_config`).
- When `state_dir` is set, the workspace migrates entirely—cached images, logs, and pidfiles will all live under the override.
- Permissions matter: if Castra cannot create the directory, `castra up` fails during `prepare_runtime_context` with a `PreflightFailed` error. Keep the override on a writable filesystem.

## `.castra`-Prefixed Paths in Configs
- Overlay paths and other VM assets that start with `.castra` are *rebased* into the resolved workspace, not the project root (`resolve_overlay_path` in `src/config.rs`).
- Examples:
  - `overlay = ".castra/devbox-overlay.qcow2"` → `<state_root>/devbox-overlay.qcow2`
  - `overlay = ".castra/api/overlay.qcow2"` → `<state_root>/api/overlay.qcow2`
- Overlays without the prefix are resolved relative to the config directory (or remain absolute). This allows a mix of generated and hand-maintained disks across VMs.

## On-Disk Layout
Every workspace follows the same structure; paths in parentheses are created on demand.

| Path | Purpose |
| --- | --- |
| `metadata/workspace.json` | Registry metadata written by `castra up` capturing project name, workspace ID, config origin, bootstrap policy, and invocation flags for multi-workspace discovery. |
| `metadata/config_snapshot.toml` | Cached copy of the resolved `castra.toml` used when the original config is unavailable (for example, if the repo moved). |
| `images/` | Cached base images. The default Alpine qcow2 is downloaded here on demand; additional qcows configured via `base_image` can also live here. |
| `logs/` | Aggregated host-side logs. Each VM writes `<vm>.log` (QEMU stdout/stderr) and `<vm>-serial.log`; bootstrap runs append JSON to `logs/bootstrap/`. Legacy `logs/bus/` directories are pruned when encountered. |
| `handshakes/` | Legacy broker ⇄ guest handshake JSON. No longer created on fresh runs; retained only for historical inspection. |
| `bootstrap/` | Per-VM staging area where bootstrap scripts and payloads are copied before upload (`assemble_blueprint`). Cleaned between runs. |
| `overlays/` | Default home for per-VM qcow2 layers derived from role names when configs omit an explicit `overlay`. Discarded after shutdown per Thread 13. |
| `<vm>.pid` | PID files written by `launch_vm`. Legacy `broker.pid` files are removed on sight. |
| `<vm>.qmp` (Unix) | QMP control sockets for cooperative shutdown, created alongside the pidfiles. |
| Other ephemeral files | Overlay qcow2 images, staging manifests, and temporary scratch directories declared by VM definitions. |

Castra creates the workspace root, `logs/`, and `images/` up front during `prepare_runtime_context`; other directories appear as subsystems need them.

## Lifecycle Touchpoints
- **`castra init`** – Scaffolds a starter config that relies on the default Alpine qcow2 (downloaded on demand) and default overlay paths under `<state_root>/overlays/`, and it prints both the global workspace and the opt-in local override so operators know where state will accumulate.
- **`castra up`** – Ensures the workspace exists, verifies host capacity, fetches the default qcow2 into `images/` if needed, and creates fresh overlays. Direct SSH session metadata replaces the old broker handshake artefacts; any lingering legacy files are pruned as part of the run. Thread 13 work guarantees overlays are disposable after shutdown (`Event::EphemeralLayerDiscarded`).
- **`castra status`** – Reads pidfiles, inspects QMP sockets, and reports whether VMs are running. Harness-published health via the session metadata stream supersedes the legacy handshake directory, which is no longer consumed.
- **`castra down`** – Walks pidfiles to coordinate cooperative shutdown, removes overlays, and reports reclaimed bytes. Shutdown remains bounded per VM while the workspace stays responsive.
  - **`castra clean`** – Deletes cached images, overlays, logs, and pidfiles under the workspace. `--workspace` targets the active state root; `--global` sweeps every child of `~/.castra/projects`. Diagnostics warn when live processes are detected unless `--force` is supplied.
- **`castra bus` / `logs` / `ports`** – Consume metadata only from within the state root, so moving the workspace (via `state_dir`) keeps these commands working automatically.

## Image Cache Notes
- Each workspace caches its own copy of the default Alpine qcow2 under `images/`. Downloads are verified via size and SHA-512; a `.sha512` sidecar records the last successful verification.
- Global cleaning (`castra clean --global`) walks every directory in `~/.castra/projects` and removes cached images/logs/pidfiles. Overlays remain untouched in global mode.
- Automation can inspect `images/alpine-minimal.qcow2` (and its `.sha512`) or listen for `Event::CleanupProgress` to audit cache state.

## Maintenance & Troubleshooting
- Always run `castra down` before manipulating the workspace manually; pidfiles and QMP sockets should disappear during shutdown. If they linger, `castra clean --workspace --force` will remove them after validating nothing is running.
- When legacy handshakes become stale or corrupted, removing `handshakes/*.json` (or running `castra clean`) tidies the workspace; harness session metadata supersedes them.
- If you relocate a project, delete or move the old workspace to avoid orphaned directories under `~/.castra/projects`. Castra will derive a new hash based on the project’s new path.
- For automation, prefer calling the library APIs (e.g., `core::project::config_state_root`) rather than hardcoding paths; this keeps tooling aligned with future schema changes in `.vizier` threads.

Castra’s `.castra` workspaces are intentionally disposable: they cache what the orchestrator needs for the next run while ensuring guest disk changes vanish on shutdown. Understanding the layout and lifecycle hooks makes it straightforward to operate, inspect, or garbage-collect these directories safely.
