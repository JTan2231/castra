# Castra

Castra is a friendly orchestration layer for lightweight QEMU-based sandboxes. It bootstraps reproducible development VMs with cached base images and a thin CLI that drives the core library APIs exposed under `castra::core`. Legacy broker/bus coordination has been removed; the harness and UI now surface direct SSH session helpers for each VM.

> Heads-up: the `castra bus` and `castra broker` commands have been removed. Migration notes, protocol guardrails, and troubleshooting tips live in `docs/migration/v0.10-brokerless-announcement.md`.

The CLI is a veneer over the library. Projects that embed Castra can disable the `cli` feature flag to depend on the core APIs without pulling in presentation code (see `docs/library_usage.md`).

Castra runs are stateless: each VM boots with a fresh overlay and all guest-side disk mutations are discarded when the VM stops. Export data you want to keep via SSH or other guest tooling before invoking `castra down`.

## Minimum Supported Rust Version

Castra targets **Rust 1.77** or later. The crate opts into the 2024 edition and relies on the toolchain updates that shipped with that release family. Install via:

```bash
rustup toolchain install 1.77.0
rustup default 1.77.0
```

Developers may use newer stable toolchains, but CI and release builds should continue to validate against the MSRV.

## Building And Testing

```bash
cargo build
cargo test
```

The binary is gated behind the `cli` feature (enabled by default). Library consumers can disable default features to compile only `castra::core`.

`castra --version` now prints the semantic version and the short git commit hash when the build runs inside a git checkout. When VCS metadata is unavailable the CLI falls back to the plain semantic version.

## Agent Sessions

`castra up` launches VMs and prepares agent wrappers that live alongside `vm_commands.sh`. The harness emits metadata describing SSH endpoints, recommended environment variables, and resolved bootstrap scripts so the UI can present direct sessions per VM. Use `vm_commands.sh list` to see active runs and `vm_commands.sh send` / `vm_commands.sh launch_subagent` to interact with guest runtimes without relying on an in-VM steward service.

## Documentation

- `docs/library_usage.md` explains how to drive Castra from another crate.
- `docs/RELEASING.md` captures the release checklist.
- `docs/migration/v0.10-brokerless-announcement.md` highlights the broker deprecation and the move from a brokered tunnel to direct SSH session helpers.

## Chat Transcripts

Launching `castra-ui` records every chat entry to `<workspace_root>/.castra/transcripts/chat-<session_id>.jsonl`. Each JSON line captures the session identifier, monotonic `sequence`, UTC `recorded_at`, the UI `display_timestamp`, `speaker`, `kind` (lowercased message kind), `text`, and whether the entry was `expanded_by_default`. Rotate the UI or tooling that consumes transcripts toward this directory to replay a complete operator timeline.

## Status JSON

Running `castra status` (and `castra down`/`castra ports`) without `--config` now inspects every active workspace discovered under `~/.castra/projects` and any local `.castra/` state roots. Results are grouped per project with headers such as `=== demo-workspace (demo-1234abcd) ===`; pass `--workspace <id>` to narrow the view to a single entry. `castra status --json` returns the same reachability view rendered by the table. The `reachable` flag reports whether any VM in the project is currently running and never blocks on a live network probe.

VM health and connectivity now surface through bootstrap events, harness metadata, and direct SSH session helpers. The legacy broker handshake directory (`<state_root>/handshakes`) is no longer produced; migrate tooling to the harness metadata surfaces described in `vizier-removal/IMPLEMENTATION_PLAN.md`.
