Thread: 20 — Harness ↔ Core integration (Vizier-first)

Why (tension): The user interacts through the Codex harness. We must expose a single, authoritative event stream that merges core operation events with per-VM SSH observations, eliminating the need for a shared bus.

Desired behavior (product-level):
- Starting an Up via the harness yields a unified event stream: core lifecycle + vizier.ssh events (connecting, connected, output, failed, disconnected) per VM.
- The vizier establishes SSH connections directly to each VM using plan information surfaced by the core (e.g., host/user/port/identity) and maintains them during bootstrap and subsequent commands.
- No broker/bus processes are launched. All remote command/control flows over SSH.

Acceptance criteria (observable):
- From a sample session: Harness emits a version preamble then a sequence including vm lifecycle + vizier.ssh.* events. Consumers (UI/tests) can render VM output and status live.
- Failure modes: transient SSH errors are surfaced with remediation_hint; terminal failures produce a summarized error and clean disconnects. Retries are possible without process leaks.
- Performance: establishing N SSH sessions (N up to the VM count in example projects) completes within a bounded time (e.g., 5s per VM under localhost lab conditions).

Anchors (pointer-level):
- Prior art: castra-ui/src/ssh/mod.rs (SshManager, handshake banner, output streaming semantics).
- Harness surfaces: castra-harness/src/{session.rs, runner.rs, stream.rs, events.rs, translator.rs}.
- Core surfaces for plan info: castra-core/src/core/events.rs (BootstrapPlanSsh), castra-core/src/app/up.rs.

Open choices (document, do not lock):
- Whether to move SshManager into the harness crate vs reuse via a shared module. Keep implementation open; ensure the contract and event mapping are stable.

Verification:
- Golden tests for harness event stream including vizier.ssh family; exercise both success and failure flows.Progress (evidence):
- castra-ui now pumps HarnessEvent into AppState via pump_codex/pump_vizier; Vizier command/system outputs are rendered distinctly and token usage tallied.
- SshManager present in UI; vm_commands.sh upgraded with `--wait` streaming and `view-output` helper to fetch outputs from remote runs.

What’s still needed to meet acceptance:
- Unified agent-addressed event stream originated in the harness (not UI) that includes vizier.ssh.* family; current SshManager remains UI-side.
- Golden tests for harness event stream (success/failure flows) are not present.
- Explicit preamble/versioning on the stream not yet verified end-to-end.
- Performance target: document and measure N-SSH session setup bounds in harness tests.

Open choice restated:
- Whether to migrate SshManager capabilities into harness vs shared module; keep contract stable either way.

Anchors reaffirmed:
- castra-harness/src/{session.rs,runner.rs,stream.rs,events.rs,translator.rs}
- castra-ui/src/{app/mod.rs,ssh/mod.rs,state/mod.rs}
- castra-core surfaces for plan info remain unchanged.

---

