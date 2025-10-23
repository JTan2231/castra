# Minimal Bootstrap Pipeline Example

This example demonstrates a single-VM bootstrap configuration that exercises the flags and metadata most teams rely on. Point `castra` at this directory to see the post-boot pipeline stages described in `docs/BOOTSTRAP.md` in action, with plenty of inline commentary that calls out how each knob contributes to the run.

## Layout Cheat Sheet
- `castra.toml` – heavily annotated project definition with global defaults plus one VM.
- `bootstrap/run.sh` – guest-side shell script; mirrors the verification hooks configured in the TOML.
- `bootstrap/payload/` – optional files copied alongside the script; this example carries a tiny `.env`.
- `bootstrap/bootstrap.toml` – metadata the runtime merges on top of the main config right before execution (SSH overrides, handshake alias, extra environment variables).
- `README.md` *(you are here)* – narrative walkthrough with suggested commands.

## Configuration Highlights
- `[bootstrap]` establishes global defaults (handshake timeout, remote working directory, shared environment). Per-VM values inherit from these defaults unless overridden.
- `[[vms]]` names the VM, forwards port 22 to localhost:2222, and defines the per-VM bootstrap stanza (script, payload, verification hooks, tighter handshake timeout).
- `[vms.bootstrap.env]` layers per-VM environment variables on top of the shared ones; metadata adds one more (`INSTANCE_ID`) so you can see the merge order in action.
- `bootstrap/bootstrap.toml` introduces `handshake_identity = "alpine"`. The bundled image reports that identity to the broker, while Castra would otherwise wait for the expanded VM name (`devbox-0`). The alias ensures the handshake step succeeds before any SSH work begins.
- The guest script writes `state/bootstrap-user` and `state/app-name`. The verify checks configured in TOML insist on seeing those artifacts, making the run fail loudly if the script goes missing or exits early.

### Required vs Optional Keys
- Required: `version`, `[project].name`, and `[[vms]].name`. Without these the loader errors before planning or running.
- Optional but commonly set: resource sizing (`cpus`, `memory`), image paths (`base_image`, `overlay`), `[bootstrap]` defaults, `[[vms.port_forwards]]`, and every field under `[vms.bootstrap]`. Each has sensible fallbacks (Alpine base image, generated overlay paths, 2 CPUs, 2048 MiB, default remote dir `/tmp/castra/<vm>`).
- Optional extras: metadata in `bootstrap.toml`, payload contents, verify hooks, or handshake aliases. Omit any of them if your runner does not need them; the runtime will surface the derived values in the plan either way.

## Before You Run
1. Ensure `alpine-x86_64.qcow2` exists beside `castra.toml`. The repo ships a demo image; if you rebuild it, keep the handshake identity as `alpine` or update the alias accordingly.
2. Adjust the SSH metadata in `bootstrap/bootstrap.toml` to match your environment. The defaults target `root@127.0.0.1:2222` with `~/.ssh/id_ed25519`.
3. Confirm your host key has passwordless access to the guest (or update the metadata with the right identity). The bootstrap runner relies exclusively on key-based auth.

## See The Plan First
Run a dry run to sanity-check the resolved policy, timeouts, SSH target, assets, and environment merge:
   ```bash
   cargo run -- up --plan \
     --config examples/minimal-bootstrap/castra.toml \
     --bootstrap=always
   ```
   Key items to look for:
   - `handshake wait: 30s` (per-VM value overriding the global 45-second deadline).
   - `ssh: root@127.0.0.1:2222` (derived from metadata + port forwarding).
   - `env keys: ... INSTANCE_ID` (shows the metadata key merged on top of the shared set).

## Launch The VM And Run The Pipeline
   ```bash
   cargo run -- up \
     --config examples/minimal-bootstrap/castra.toml \
     --bootstrap=always
   ```
   Watch for:
   - `wait-handshake` finishing almost immediately thanks to the alias.
   - `transfer` staging the script and payload into the state root.
   - `apply` streaming the script output shown in `bootstrap/run.sh`.
   - `verify` re-checking the sentinel files the script created.

   TTY mode concludes with a one-line summary that mirrors the JSON run log at
   `~/.castra/projects/bootstrap-demo-*/logs/bootstrap/devbox-0-<timestamp>.json`.

## Script Expectations
- `CASTRA_VM`, `CASTRA_RUN_ID`, and `CASTRA_PAYLOAD_DIR` are exported by the host runtime (see `src/core/bootstrap.rs`).
- Custom environment keys from `[bootstrap.env]`, `[vms.bootstrap.env]`, and metadata are available while the script runs.
- Reporting a no-op or failure can be done by printing `CASTRA_NOOP` or `CASTRA_ERROR: reason` on stdout, matching the sentinel handling in `execute_remote`.

## Troubleshooting Tips
- Handshake timeout? Double-check `handshake_identity` matches whatever the guest reports to the broker (`/etc/conf.d/castra-handshake`). You can tail `~/.castra/.../logs/broker.log` to see the raw records.
- SSH failures? Confirm the port forward is active (`lsof -i :2222`) and that your key matches the metadata.
- Verification failures? Inspect `/var/tmp/castra/devbox/state` inside the guest to see what the script produced.
