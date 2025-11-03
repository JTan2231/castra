Thread: 20/12 — In-VM Vizier service bootstrap and SSH tunnel control

Why (tension): The orchestrating Vizier must reside inside each VM to avoid fragile cross-VM SSH orchestration from the host/UI. We need a reliable, testable way to start, keep alive, and converse with that in-VM Vizier via SSH.

Desired behavior (product-level):
- After `castra up`, each VM runs a Vizier process that:
  - Emits a versioned handshake banner on attach.
  - Reads user input/events from stdin and writes outputs/system/status to stdout/stderr with stable framing compatible with the harness stream.
  - Can be restarted idempotently by the bootstrap pipeline without duplicate instances.
- The harness creates one SSH session per VM that attaches to this Vizier and forwards user input; drops/reconnects are reported.

Acceptance criteria (observable):
- `up` leads to: SSH attach → handshake banner within 2s → harness emits vizier.remote.connected with version fields.
- Forwarded input echoed/acknowledged within 150ms in localhost lab; outputs stream continuously under load without interleaving corruption.
- If the SSH session drops, harness emits reconnect_attempt/established within exponential backoff; no zombie Vizier processes are left behind.
- `--plan` indicates Vizier status per VM (Start|Restart|Healthy/NoOp|Unavailable) without side effects.

Pointers (anchor-level):
- Bootstrap entrypoints: castra-core/src/core/bootstrap.rs; app/up.rs for post-boot steps.
- Harness stream/orchestration: castra-harness/src/{session.rs,stream.rs,events.rs,runner.rs}.
- UI consumer: castra-ui/src/app/mod.rs pump_vizier; ssh prior art in castra-ui/src/ssh/mod.rs.

Safety/correctness notes (implementation-open):
- Keep supervision strategy open (systemd, dumb init, or custom loop); contract is the stable stdin/stdout handshake and framing.
- Ensure only one Vizier instance per VM; restarts should be graceful when possible.
- Log paths in-VM must be discoverable via events for troubleshooting.