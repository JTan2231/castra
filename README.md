# Castra

Castra is a friendly orchestration layer for lightweight QEMU-based sandboxes. It bootstraps reproducible development VMs with cached base images, a broker for host↔guest coordination, and a thin CLI that drives the core library APIs exposed under `castra::core`.

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

## Broker-Only Mode

Use `castra up --broker-only` to start just the TCP broker for bus testing. The flag prepares the workspace directories, verifies the broker port is free, and launches the listener without touching VM overlays or running bootstrap steps. If guests are already running, the command leaves them in place and surfaces a warning.

## Documentation

- `docs/library_usage.md` explains how to drive Castra from another crate.
- `docs/RELEASING.md` captures the release checklist.

## Status JSON

`castra status --json` returns the same reachability view rendered by the table. The `reachable` flag stays `true` while the freshest guest handshake is at most 45 seconds old and flips to `false` once that cache ages out; the value is derived from on-disk records and never blocks on a live network probe. `last_handshake_age_ms` reports the age of that freshest handshake in milliseconds (omitted when no guest has connected).

Every handshake produces both a deterministic broker log line and a JSON event appended to `<state_root>/handshakes/handshake-events.jsonl`. Each entry records the VM name, normalized capabilities, session outcome (`granted`, `denied`, or `timeout`), and an optional reason. The event timestamp matches the value used for reachability calculations.

Example log lines emitted by the broker:

```text
[host-broker] 12:00:00 INFO handshake ts=1700000123 vm=devbox remote=127.0.0.1:41000 capabilities=[bus-v1] session_kind=guest session_outcome=granted
[host-broker] 12:00:01 INFO handshake ts=1700000124 vm=host remote=127.0.0.1:41001 capabilities=[bus-v1] session_kind=guest session_outcome=denied reason=reserved-identity
[host-broker] 12:00:05 INFO handshake ts=1700000128 vm=127.0.0.1:41002 remote=127.0.0.1:41002 capabilities=[-] session_kind=guest session_outcome=timeout reason=read-timeout
```

Corresponding JSON events:

```json
{"timestamp":1700000123,"vm":"devbox","capabilities":["bus-v1"],"session_kind":"guest","session_outcome":"granted","remote_addr":"127.0.0.1:41000"}
{"timestamp":1700000124,"vm":"host","capabilities":["bus-v1"],"session_kind":"guest","session_outcome":"denied","reason":"reserved-identity","remote_addr":"127.0.0.1:41001"}
{"timestamp":1700000128,"vm":"127.0.0.1:41002","capabilities":[],"session_kind":"guest","session_outcome":"timeout","reason":"read-timeout","remote_addr":"127.0.0.1:41002"}
```

Guests that attempt to impersonate the reserved `host` identity without presenting the `host-bus` capability are denied with `reason=reserved-identity`. Idle connections that fail to present a handshake before the socket deadline record `session_outcome=timeout` with `reason=read-timeout`.
