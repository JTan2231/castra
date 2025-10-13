# Castra

Castra is a friendly orchestration layer for lightweight QEMU-based sandboxes. It bootstraps reproducible development VMs with managed images, a broker for host↔guest coordination, and a thin CLI that drives the core library APIs exposed under `castra::core`.

The CLI is a veneer over the library. Projects that embed Castra can disable the `cli` feature flag to depend on the core APIs without pulling in presentation code (see `docs/library_usage.md`).

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

## Documentation

- `docs/library_usage.md` explains how to drive Castra from another crate.
- `docs/RELEASING.md` captures the release checklist.

## Status JSON

`castra status --json` returns the same reachability view rendered by the table. The `reachable` flag stays `true` while the freshest guest handshake is at most 45 seconds old and flips to `false` once that cache ages out; the value is derived from on-disk records and never blocks on a live network probe. `last_handshake_age_ms` reports the age of that freshest handshake in milliseconds (omitted when no guest has connected).

Every handshake also emits a structured JSON line under `<state_root>/handshakes/handshake-events.jsonl` recording the VM name, sorted capabilities, session outcome (`granted` or `denied`), and any denial reason. The timestamp stored in the event matches the value used for reachability calculations.
