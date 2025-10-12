# Stage 0 Baseline – Castra Library Enablement

This memo records the baseline facts requested in Stage 0 of `LIBRARY.md`. It captures what the current CLI-driven implementation looks like before the extraction work begins.

## Architecture Overview

- **Entry flow** – `src/main.rs` parses arguments via `cli::Cli` (Clap) and dispatches into the `app::*` handlers (`handle_init`, `handle_up`, …). These handlers are tightly coupled to Clap argument structs such as `InitArgs` and `UpArgs`.
- **Presentation leakage** – Business logic sits inside the `app` tree. Functions emit user-facing text directly via `println!`/`eprintln!` (e.g. `app::init`, `app::up`, `app::status::print_status_table`, `app::logs`). Diagnostics are surfaced as strings baked into `CliError`.
- **Configuration** – `src/config.rs` owns parsing and validation into `ProjectConfig`, but it calls back into `app::project` for discovery helpers and default scaffolding. That module also prints warnings immediately.
- **Runtime** – `app::runtime` glues together state preparation, managed image acquisition, host checks, and VM/broker lifecycle. It relies on `CliResult`/`CliError` and prints warnings plus progress from inside helper functions.
- **Managed artifacts** – `src/managed` contains the HTTP-backed managed image downloader and emits progress events via return values that `app::runtime` currently prints synchronously.
- **Error propagation** – `CliError` is defined in `src/error.rs` alongside the `exit_code()` helper. The type name (`CliError`) leaks CLI semantics into modules that should stay headless (e.g. `config.rs`, `managed`).
- **Process lifecycle** – `app::runtime::start_broker`, `stop_broker`, `launch_vm`, and `stop_vm` create/remove PID files inside the state root; `status` and `down` inspect those PIDs to determine state.

## CLI Transcript Baselines

Captured on commit prior to library extraction, using `cargo run --` inside the repo and a sample project rooted at `target/baseline_sample`.

### `castra init`

````text
$ cargo run -- init --output target/baseline_sample/castra.toml --force
✔ Created castra project scaffold.
  config  → target/baseline_sample/castra.toml
  state   → /Users/joey/.castra/projects/baseline-sample-41ed0f99cd25d341
  local   → target/baseline_sample/.castra (opt-in via [project].state_dir)

Next steps:
  • Update `base_image` or set `managed_image` in the config to choose your base disk.
  • Run `castra up` once the image is prepared.
````

### `castra status`

````text
$ cargo run -- status --config target/baseline_sample/castra.toml
Project: baseline_sample (target/baseline_sample/castra.toml)
Config version: 0.1.0
Broker endpoint: 127.0.0.1:7070
Broker process: offline (run `castra up`).

VM      STATE      CPU/MEM  UPTIME  BROKER   FORWARDS
devbox  stopped  2/2048MiB       —  offline  2222->22/tcp, 8080->80/tcp

Legend: BROKER reachable = host broker handshake OK; waiting = broker up, guest not connected; offline = listener not running.
States: stopped | starting | running | shutting_down | error
Exit codes: 0 on success; non-zero if any VM in error.
````

### `castra ports`

````text
$ cargo run -- ports --config target/baseline_sample/castra.toml
Project: baseline_sample (target/baseline_sample/castra.toml)
Config version: 0.1.0
Broker endpoint: 127.0.0.1:7070
(start the broker via `castra up` once available)

Declared forwards:
  VM       HOST  GUEST  PROTO  STATUS
  devbox   2222     22  tcp  declared
  devbox   8080     80  tcp  declared
````

### `castra logs`

````text
$ cargo run -- logs --config target/baseline_sample/castra.toml --tail 5
Tailing last 5 lines per source.

[host-broker] (log file not created yet)

[vm:devbox:qemu] (log file not created yet)

[vm:devbox:serial] (log file not created yet)
````

### `castra down`

````text
$ cargo run -- down --config target/baseline_sample/castra.toml
→ devbox: already stopped.
→ broker: already stopped.
No running VMs or broker detected.
````

### `castra up`

````text
$ cargo run -- up --config target/baseline_sample/castra.toml
Error: Preflight failed: Failed to create castra state directory at /Users/joey/.castra/projects/baseline-sample-41ed0f99cd25d341: Operation not permitted (os error 1)
````

### Hidden `castra broker`

Launching the hidden broker command requires a long-lived process; no transcript captured here to avoid hanging the harness. Current implementation simply prints nothing and blocks while serving TCP connections.

## Configuration & Filesystem Expectations

- `ProjectConfig` fields: `file_path`, semantic `version`, human `project_name`, `vms` (vector of `VmDefinition`), `state_root` (defaults under `~/.castra/projects/<slug>-<hash>`), `workflows`, `broker`, and accumulated parse `warnings`.
- VM overlays default under `.castra/<vm>-overlay.qcow2` relative to the config, or under the computed `state_root` when `.castra` paths are used.
- Managed images live under `<state_root>/images` and broker/log directories live under `<state_root>/logs`.
- PID files: `<state_root>/<vm>.pid` for each VM, plus `<state_root>/broker.pid`; log files mirror the PID naming scheme.
- Helper functions that already return structured data: `config::load_project_config`, `project::resolve_config_path`, `status::collect_vm_status`, `runtime::ensure_vm_assets` (returns preparation record). Many helpers still emit textual diagnostics inline.

## Broker & VM Lifecycle (Current State)

- `runtime::prepare_runtime_context` ensures directories exist and locates binaries.
- `runtime::start_broker` launches the hidden broker binary via `Command`, writes broker PID/log files, and records `broker.pid`.
- `runtime::launch_vm` spawns QEMU with `Command`, stores PID files, and writes stdout/stderr logs.
- Status interrogation reads PID files and checks for live processes via `/proc` inspection (`libc::kill(pid, 0)`).
- Shutdown clears PID files and issues ACPI shutdown via QMP; fallbacks rely on `kill` signals when processes do not exit.

## Test Coverage Notes

- `cargo test` currently fails under the sandbox: `app::runtime::tests::ensure_port_is_free_detects_conflicts` attempts to bind privileged ports and `app::project::tests::load_or_default_project_synthesizes_when_missing` expects filesystem state outside the sandbox.
- Existing tests focus on parsing (`config.rs`), CLI argument parsing (`cli.rs`), and broker log formatting. No tests exercise the high-level workflows end-to-end or assert CLI text output, which will be a gap once the library API is introduced.

