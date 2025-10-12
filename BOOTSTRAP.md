# Castra Post-Boot Bootstrap Architecture

This document describes how to layer reproducible post-boot provisioning on top
of Castra’s managed Alpine images. It follows the `.vizier` guidance for Thread 10
(*Seamless Alpine Bootstrap*), building a path from the moment QEMU starts to a
guest that applies a Nix flake driven environment immediately after it comes
online.

The audience is anyone wiring up `castra up` for a project that expects a
package-managed developer stack waiting inside the VM. The plan keeps managed
image guarantees intact (checksums, boot profiles, broker freshness) while
adding an opinionated bootstrap pipeline that stays idempotent and observable.

## Objectives
- Respect managed image verification: downloads must continue to verify source
  checksums and log “verified source checksums” events (Thread 10 contract).
- Trigger guest provisioning as soon as a fresh broker handshake declares the
  VM reachable (Thread 3 contract).
- Drive provisioning from a Nix flake stored with project sources so changes
  are auditable.
- Keep the workflow scriptable: every operation (`castra status`, `nix build`,
  `ssh` execution) is CLI friendly with JSON output where available.
- Remain portable: the same bootstrap scripts must run on macOS and Linux hosts
  with only POSIX shell dependencies plus Nix/SSH.

## Boot Timeline Overview
The bootstrap flow slots into Castra’s existing lifecycle:

1. **Host preflight (`workflows.init`)**  
   Host scripts create overlays, fetch managed images, and build the Nix flake
   into a distributable form.
2. **Managed image verification**  
   `castra up` acquires managed artifacts, verifies sizes and SHA-256 hashes,
   and emits the log events Thread 10 requires.
3. **QEMU launch with optional boot profile**  
   When a managed image advertises a kernel/initrd combo, QEMU launches with
   `-kernel/-initrd/-append` arguments and logs “applied boot profile”.
4. **Guest first boot**  
   Alpine boots, starts `sshd` (enabled by overlay or host script), and runs a
   guest-side agent that contacts the broker with `hello vm:<name>`.
5. **Broker handshake**  
   The broker records the handshake timestamp under
   `<state_root>/handshakes/<vm>.json`. `castra status --json` now reports
   `reachable=true` and `last_handshake_age_ms≈0`.
6. **Host bootstrap daemon**  
   A host-side watcher notices the fresh handshake and executes the Nix
   bootstrap script over SSH.
7. **Guest provisioning**  
   Inside the guest the Nix flake is copied (or fetched), `nix profile install`
   or `nix run` applies packages/users/services, and a stamp file marks the run.
8. **Steady state**  
   Subsequent handshakes keep freshness under 45 s; rerunning `castra up`
   re-applies bootstrap if the Nix flake changed.

## Repository Layout

```
CAS-project-root/
├── castra.toml
├── nix/
│   ├── flake.nix
│   └── profiles/
│       ├── default.nix
│       └── devbox.nix
├── scripts/
│   ├── bootstrap-host.sh
│   ├── bootstrapd.sh
│   └── guest-bootstrap.sh
└── BOOTSTRAP.md
```

- `castra.toml` references the scripts via `[workflows].init`.
- `nix/` hosts the flake that defines guest environments.
- `scripts/bootstrap-host.sh` runs before the VM boots to build artifacts and
  launch the bootstrap daemon.
- `scripts/bootstrapd.sh` waits for broker handshakes and triggers guest
  provisioning.
- `scripts/guest-bootstrap.sh` executes inside the VM (over SSH) and applies
  the flake.

## Nix Flake Shape

An example `nix/flake.nix` that targets Alpine userspace but installs a fixed
toolchain via `nix profile`:

```nix
{
  description = "Castra devbox profile";
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-24.05";

  outputs = { self, nixpkgs }: {
    packages.aarch64-darwin.devbox =
      let pkgs = import nixpkgs { system = "x86_64-linux"; };
      in pkgs.buildEnv {
        name = "castra-dev-env";
        paths = with pkgs; [
          git
          just
          neovim
          nodejs_20
          rustup
        ];
      };

    packages.x86_64-linux.devbox =
      let pkgs = import nixpkgs { system = "x86_64-linux"; };
      in pkgs.buildEnv {
        name = "castra-dev-env";
        paths = with pkgs; [
          git
          just
          neovim
          nodejs_20
          rustup
        ];
      };

    apps.x86_64-linux.activate = {
      type = "app";
      program = "${self.packages.x86_64-linux.devbox}/bin/activate";
    };
  };
}
```

Key ideas:
- Build host-side (`nix build .#devbox`) so `bootstrap-host.sh` can stash the
  closure under the Castra state root, making guest installs deterministic.
- Expose an `apps.<system>.activate` entry point that runs any additional
  configuration (user creation, dotfiles sync).
- Keep `flake.lock` in source control so bootstrap is reproducible.

## Host Workflow Wiring

`castra.toml` connects scripts to lifecycle hooks:

```toml
[workflows]
init = [
  "scripts/bootstrap-host.sh",
  "scripts/bootstrapd.sh --once"
]

[[vms]]
name = "devbox"
managed_image.name = "alpine-minimal"
managed_image.version = "v1"
overlay = ".castra/devbox/overlay.qcow2"
cpus = 4
memory = "4096 MiB"

  [[vms.port_forwards]]
  host = 2222
  guest = 22
  protocol = "tcp"
```

### `scripts/bootstrap-host.sh`

Responsibilities:
1. Resolve the Castra state root (read `castra status --json` and grab
   `.project.state_root` to avoid hard-coding).
2. Ensure `nix` and `ssh` tooling exists; emit a diagnostic if missing.
3. Build the flake (`nix build ./nix#devbox`) and copy the result to
   `$STATE_ROOT/bootstrap/devbox`.
4. Stage guest artifacts:
   - Authorized keys (`bootstrap/devbox/authorized_keys`).
   - Guest-side scripts (copied from `scripts/guest-bootstrap.sh`).
5. Record a manifest file with the build output’s hash. Subsequent runs exit
   early when the hash matches to keep idempotency.

Sketch:

```sh
#!/usr/bin/env bash
set -euo pipefail

STATE_ROOT="$(castra status --json | jq -r '.project.state_root')"
TARGET="$STATE_ROOT/bootstrap/devbox"
mkdir -p "$TARGET"

if ! command -v nix >/dev/null; then
  echo "castra bootstrap: nix not found on host" >&2
  exit 1
fi

nix build ./nix#devbox --out-link "$TARGET/result"
cp scripts/guest-bootstrap.sh "$TARGET/guest-bootstrap.sh"
cp ssh_keys/devbox.pub "$TARGET/authorized_keys"

HASH="$(nix path-info --json "$TARGET/result" | jq -r '.[0].narHash')"
printf '%s\n' "$HASH" > "$TARGET/result.hash"
```

### `scripts/bootstrapd.sh`

Runs as part of `workflows.init` but typically backgrounds itself:

```sh
#!/usr/bin/env bash
set -euo pipefail

STATE_ROOT="$(castra status --json | jq -r '.project.state_root')"
VM="$1"
STAMP_DIR="$STATE_ROOT/bootstrap/$VM/stamps"
HANDSHAKE="$STATE_ROOT/handshakes/$VM.json"
mkdir -p "$STAMP_DIR"

while true; do
  if [ -f "$HANDSHAKE" ]; then
    AGE_MS="$(castra status --json | jq -r '.last_handshake_age_ms // 999999')"
    if [ "$AGE_MS" -lt 2000 ] && [ ! -f "$STAMP_DIR/$(cat "$STATE_ROOT/bootstrap/$VM/result.hash")" ]; then
      ./scripts/run-guest-bootstrap.sh "$VM"
    fi
  fi
  sleep 2
done
```

Rationale:
- Reuses Castra’s `last_handshake_age_ms` to avoid direct filesystem polling.
- Pairs bootstrap runs with the flake manifest hash so upgrades retrigger the
  guest script.
- Can run in `--once` mode for CI (exit after successful bootstrap).

`scripts/run-guest-bootstrap.sh` is a thin wrapper around `ssh` that feeds the
guest script with the prebuilt flake path:

```sh
ssh -i ssh_keys/devbox \
    -o StrictHostKeyChecking=accept-new \
    dev@localhost -p 2222 \
    'sudo /opt/castra-bootstrap/guest-bootstrap.sh /opt/castra/devbox'
```

## Guest Bootstrap Agent

The managed Alpine image needs two additions:
1. **Broker handshake helper** – a shell script installed as `/usr/local/bin/castra-handshake`
   that runs at boot:

   ```sh
   #!/bin/sh
   set -eu
   HOST_IP=10.0.2.2
   PORT=7070
   VM_NAME="$(hostname)"
   printf 'hello vm:%s\n' "$VM_NAME" | busybox nc -w 5 "$HOST_IP" "$PORT"
   ```

   Place it behind an OpenRC service (`/etc/init.d/castra-handshake`) and add it
   to the default runlevel (`rc-update add castra-handshake default`) so the VM
   announces itself every boot.

2. **Bootstrap runner** – `/opt/castra-bootstrap/guest-bootstrap.sh`, copied in
   by the host before SSH executes it. This script:
   - Ensures Nix is installed (`apk add nix` guarded by a stamp).
   - Copies the host-built closure from `/opt/castra/devbox` (provided via `scp`).
   - Runs `nix profile install /opt/castra/devbox/result`.
   - Applies any activation hook (`/opt/castra/devbox/bin/activate`).
   - Writes `/var/lib/castra-bootstrap/<narHash>` to mark success.

To keep the managed image immutable, the host script scp’s both files into the
overlay on first bootstrap. Subsequent boots pick them up from the overlay.

## Sequence Summary

```
Host (castra up)
  ├─ workflows.init → scripts/bootstrap-host.sh
  ├─ workflows.init → scripts/bootstrapd.sh (background)
  ├─ ensure_image(): verifies checksums, emits events
  ├─ start_broker(): broker listens on 127.0.0.1:7070
  └─ start_vm(): QEMU launches devbox

Guest (Alpine)
  ├─ OpenRC starts castra-handshake → broker records handshake
  └─ sshd listens on 22 (forwarded to host 2222)

Host bootstrapd
  ├─ Polls `castra status --json`
  ├─ Detects reachable=true with fresh handshake
  └─ Runs run-guest-bootstrap.sh (SSH)

Guest bootstrap script
  ├─ Installs nix if missing
  ├─ Copies /opt/castra/devbox closure
  ├─ Runs nix profile install
  ├─ Executes activation hook
  └─ Writes stamp hash → idempotent
```

## Observability
- `castra status --json` exposes `last_handshake_age_ms` and the last VM name.
  Host scripts should log the values they see to ease debugging.
- Broker logs (`<state_root>/logs/broker.log`) include “handshake success” lines.
- Bootstrap scripts must emit their own logs under
  `<state_root>/logs/bootstrap/<vm>.log`; capture stdout/stderr from SSH and
  rotate files per run.
- A failed bootstrap should leave the stamp absent so the daemon retries, but it
  must also write a failure marker (`failed-<timestamp>.log`) for inspection.

## Failure Modes and Recovery
- **Broker offline** – `castra status` reports `reachable=false`; bootstrapd
  waits until the broker comes back. Ensure the OpenRC service reconnects after
  network restarts.
- **Checksum mismatch** – `castra up` aborts before boot. Fix the flake or clear
  the cache per the managed image diagnostics.
- **SSH unreachable** – bootstrapd retries; investigate port forwards, firewall,
  or user credentials.
- **Nix install failure** – guest script writes `/var/lib/castra-bootstrap/failed`
  with logs. Host surfaces the error in `bootstrap/<vm>/last-error.log`.

## Next Steps
1. Package the guest handshake helper and bootstrap script into a small tarball
   so `bootstrap-host.sh` can push them atomically.
2. Integrate with future lifecycle hooks (Thread 2) to trigger teardown scripts
   before `castra down`.
3. Extend docs (`docs/library_usage.md`) with a short section that references
   this architecture for library consumers who handle bootstrap manually.
4. Consider adding a first-class `castra bootstrap` command that replaces
   `scripts/bootstrapd.sh` once the design stabilizes.
