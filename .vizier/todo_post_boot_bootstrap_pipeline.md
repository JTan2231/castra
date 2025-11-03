---
Thread 12 — Post‑boot bootstrap pipeline (canonical)

Two tracks consolidated: UX polish + stamp persistence/idempotence.

Problem
- Functionally solid but hard to operate: flags are scattered, output is noisy, and common failures lack clear remediation. Idempotence stamps need finalized persistence and rerun semantics.

Scope (product level)
1) Simplify/clarify CLI affordances
   - Single flag: `--bootstrap=auto|always|skip` with optional per‑VM CSV overrides `--bootstrap vmA=skip,vmB=always`.
   - `--plan` dry‑run prints what would bootstrap and why (Success|NoOp|Skipped rationale) without side effects.
   - Help text includes concise examples for single VM, multiple VMs, and JSON streaming use.

2) Human‑friendly progress and summaries
   - During `up`, render a compact per‑VM progress line (Waiting for handshake → Running steps → Completed/NoOp/Skipped/Failed) in TTY mode only.
   - Completion prints a one‑line outcome per VM with total duration and a short next‑step hint on failure.
   - JSON schema remains unchanged.

3) Actionable errors and remediation hints
   - Map handshake timeout, SSH auth failure, missing artifact, and generic channel errors to 3–5 friendly hints; always include a durable log path.

4) Idempotence stamp persistence and reruns
   - Persist stamps under the state root keyed by (base_image_hash, bootstrap_artifact_hash); Success writes; NoOp does not mutate state.
   - Reruns on unchanged inputs yield NoOp without side effects; Always forces execution despite stamps; Disabled yields Skipped.
   - Per‑VM overrides take precedence over global; conflicts rejected preflight with a clear error.

Acceptance Criteria
- `castra up --bootstrap=auto|always|skip` works with per‑VM CSV overrides; conflicts fail preflight.
- `--plan` produces deterministic per‑VM summaries with exit code 0 when valid; no side effects.
- Non‑JSON TTY shows compact progress and final one‑liners; `--json` output is unchanged and machine‑stable.
- Failures show a concise hint + durable log path.
- Stamps are durable and discoverable; unchanged reruns are NoOp; forced runs update logs and stamps; outcomes returned in input order; status stays responsive.

Pointers
- app/up.rs; src/cli.rs (flags/help)
- docs/BOOTSTRAP.md (Quick Start, Troubleshooting, examples)
- src/core/status.rs; src/core/reporter.rs (events/logs)
- State‑root conventions (stamp layout)

Notes (safety/correctness)
- Stamp writes are atomic and only on Success; concurrent per‑VM runs do not race stamp writes; a single durable failed‑run log is kept on BootstrapFailed.
---Add: In-VM Vizier launch as part of bootstrap

- On successful bootstrap, the VM must have a long-lived Vizier process running and ready to accept stdin/stdout control over SSH.
- Health check step verifies the Vizier handshake responds within 2s and reports its version.
- Failure to start/handshake surfaces a remediation hint and durable log path; stamp is not written on failure.
- `--plan` output annotates whether Vizier is expected to (Re)start or is already healthy (NoOp).

---

Augment: In-VM Vizier health and plan annotations
- After bootstrap, perform a Vizier handshake check (≤2s) and surface vm_vizier_version.
- `--plan` shows Vizier action per VM: Start | Restart | Healthy (NoOp) | Unavailable.
- Stamp policy: do not write idempotence stamp on failed Vizier start/handshake; only on Success.
- Errors include remediation_hint and durable in-VM log path.
- Pointer: VIZIER_REMOTE_PROTOCOL.md for handshake fields.


---

