# Minimal Bootstrap Pipeline Example

This example demonstrates a single-VM bootstrap configuration that exercises the flags and metadata most teams rely on. Point `castra` at this directory to see the post-boot pipeline stages described in `docs/BOOTSTRAP.md` in action, with plenty of inline commentary that calls out how each knob contributes to the run.

## Layout Cheat Sheet
- `castra.toml` – heavily annotated project definition with global defaults plus one VM.
- `bootstrap/run.sh` – guest-side shell script; mirrors the verification hooks configured in the TOML.
- `bootstrap/payload/` – optional files copied alongside the script; this example carries a tiny `.env`.
- `bootstrap/bootstrap.toml` – metadata the runtime merges on top of the main config right before execution (SSH overrides and extra environment variables).
- `README.md` *(you are here)* – narrative walkthrough with suggested commands.

## Configuration Highlights
- `[bootstrap]` establishes global defaults (handshake timeout, remote working directory, shared environment). Per-VM values inherit from these defaults unless overridden.
- `[[vms]]` names the VM, forwards port 22 to localhost:2222, and defines the per-VM bootstrap stanza (script, payload, verification hooks, tighter handshake timeout).
- `[vms.bootstrap.env]` layers per-VM environment variables on top of the shared ones; metadata adds one more (`INSTANCE_ID`) so you can see the merge order in action.
- `bootstrap/bootstrap.toml` carries the SSH overrides and extra environment keys merged into the run. The runtime now treats SSH reachability as readiness, so no handshake alias is required.
- The guest script writes `state/bootstrap-user` and `state/app-name`. The verify checks configured in TOML insist on seeing those artifacts, making the run fail loudly if the script goes missing or exits early.

### Required vs Optional Keys
- Required: `version`, `[project].name`, and `[[vms]].name`. Without these the loader errors before planning or running.
- Optional but commonly set: resource sizing (`cpus`, `memory`), image paths (`base_image`, `overlay`), `[bootstrap]` defaults, `[[vms.port_forwards]]`, and every field under `[vms.bootstrap]`. Each has sensible fallbacks (Alpine base image, generated overlay paths, 2 CPUs, 2048 MiB, default remote dir `/tmp/castra/<vm>`).
- Optional extras: metadata in `bootstrap.toml`, payload contents, verify hooks, or legacy handshake aliases for guests that still emit Vizier metadata. Omit any of them if your runner does not need them; the runtime will surface the derived values in the plan either way.

## Before You Run
1. Ensure `alpine-x86_64.qcow2` exists beside `castra.toml`. The repo ships a demo image you can reuse or replace.
2. Adjust the SSH metadata in `bootstrap/bootstrap.toml` to match your environment. The defaults target `root@127.0.0.1:2222` without an explicit identity; uncomment the sample line if your guest requires a key and remember to use an absolute path because `~` is not expanded.
3. Confirm the guest allows passwordless access for the configured user (the bundled Alpine image does). If it requires credentials, add the appropriate identity or other SSH options to the metadata before running.
4. If a previous run fails mid-flight, `castra down` or `castra clean` will reset the workspace until the automated cleanup improvements land.

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
   - `wait-handshake` completing once SSH reports ready (no broker alias required).
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
- Handshake timeout? Verify the guest is listening on the forwarded SSH port (e.g. `lsof -i :2222`) and that the credentials in `bootstrap/bootstrap.toml` are correct. The wait stage now leans entirely on SSH reachability.
- SSH failures? Confirm the port forward is active (`lsof -i :2222`) and that your key matches the metadata.
- Verification failures? Inspect `/var/tmp/castra/devbox/state` inside the guest to see what the script produced.
