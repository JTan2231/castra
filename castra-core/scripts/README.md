# Castra Guest Bus Scripts

These scripts implement the guest-side baseline described in `../VM_BUS_REQUIREMENTS.md` so that the Alpine VM image can ship with a working bus agent.

## Contents

- `castra-bus-common.sh` – shared helpers for resolving broker config, performing the handshake, sending framed JSON, and handling publish/heartbeat ACKs.
- `castra-bus-agent.sh` – long-running agent that opens a session, keeps heartbeats alive, and (optionally) forwards newline-delimited JSON frames from stdin.
- `castra-bus-publish.sh` – one-shot publisher for simple diagnostics or bootstrap hooks.
- `castra-handshake.conf.example` – sample `/etc/conf.d/castra-handshake` file documenting the environment knobs the scripts honor.

All scripts expect `bash` (for `/dev/tcp` and expansion helpers) plus core BusyBox utilities (`dd`, `od`, `sed`, `wc`). Install `bash` in the guest image and copy these files somewhere like `/usr/local/lib/castra/` during the `make-alpine-vm-image` build, then symlink or wrap them under `/usr/local/bin`.

## Suggested wiring

1. Copy `castra-handshake.conf.example` to `/etc/conf.d/castra-handshake` and tailor `BUS_VM_NAME`, host, and port after the VM definition is known.
2. Install `castra-bus-agent.sh` as an init-managed service (OpenRC, s6, or similar) to keep the session and heartbeats running boot-to-boot.
3. Use `castra-bus-publish.sh` for bootstrap hooks that need to emit readiness or status frames from early user-data scripts.

The helper library keeps the frame format, maximum size enforcement, and heartbeat cadence in sync with the broker contract so higher-level tooling can stay lean.
