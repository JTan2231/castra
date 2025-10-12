# Thread 12 — Post-boot bootstrap pipeline (Nix-driven, handshake-triggered)

Context
- Source: BOOTSTRAP.md proposes a reproducible post-boot provisioning flow layered atop managed Alpine images.
- Depends on: Thread 10 (managed image verification and optional boot profile), Thread 3 (broker reachability freshness).
- Anchors: `docs/`, managed images surfaces, broker/status fields, and example workflows via `[workflows].init` hooks in castra.toml.

Product outcome
- After `castra up`, a host-side bootstrap daemon notices a fresh broker handshake and applies a Nix flake-driven environment inside the guest over SSH, idempotently.

Acceptance criteria
- Status surface includes `reachable=true` and `last_handshake_age_ms≈0` upon fresh guest handshake (Thread 3 acceptance), enabling trigger logic.
- Managed images emit verification events for source checksums and log any applied boot profile (Thread 10 acceptance).
- Reference scripts exist (docs or examples) for `bootstrap-host.sh`, `bootstrapd.sh`, and `guest-bootstrap.sh`, with clear wiring via `[workflows].init` in `castra.toml`.
- Observability: logs captured under `<state_root>/logs/bootstrap/` and broker logs show handshake successes; failed runs leave failure markers for inspection.
- Portability: scripts rely only on POSIX shell + Nix/SSH; work on macOS and Linux hosts.
- Idempotency: bootstrap reruns only when the flake hash changes; stamps recorded in state to prevent unnecessary work.

Notes
- Keep this product-level: do not bake these scripts into the binary yet. Consider a future `castra bootstrap` first-class command once the design stabilizes.
- Cross-links: consider future reuse of the Castra Bus (Thread 13) as a trigger path once available.