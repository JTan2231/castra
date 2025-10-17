
---
Edit (stamp-free pipeline + UX polish)

Context
- Code now runs bootstrap without host-side stamps; repeated `up` attempts the pipeline again (Auto/Always) with idempotence delegated to the guest runner.
- Artifacts are discovered from per-VM script/payload config (defaults under project_root/bootstrap/<vm>/) and optional bootstrap.toml metadata. SSH is inferred from port forwards when using the default host/port.

Re-scoped Product changes
1) Simplify/clarify CLI affordances
   - Keep `--bootstrap=auto|always|skip` with per-VM CSV overrides.
   - Add `--plan` dry-run that reports intent and the resolved inputs (script path, payload presence/bytes, remote_dir, inferred SSH target) without side effects.
   - Help text shows examples for single/multi-VM and JSON streaming; explicitly note stamp-free reruns.

2) TTY progress and summaries (non-JSON only)
   - Compact per-VM progress: Waiting for handshake → Connect → Transfer → Apply → Verify → Completed/NoOp/Skipped/Failed.
   - One-line completion per VM with duration; on failure include a short hint + durable log path.
   - JSON schema remains unchanged.

3) Actionable errors and remediation hints
   - Handshake timeout, SSH auth/connectivity failure, missing script/payload, and remote verify failures map to 3–5 friendly hints. Always include log path.

4) Stamp removal confirmation (docs/tests)
   - Purge any remaining stamp references in docs/help; tests assert no stamp dirs are created and outcomes/logs omit stamps.

Acceptance Criteria
- `castra up --bootstrap=auto|always|skip` with per-VM overrides works; conflicts fail preflight with a clear message.
- `--plan` yields deterministic summaries (no side effects) and exit code 0 for valid configs.
- Non-JSON TTY shows compact progress and final one-liners; `--json` output unchanged and stable.
- Failures show concise hint + durable log path.
- Reruns in Auto/Always re-attempt full pipeline; NoOp is reported only if guest script emits the agreed sentinel (no host stamps).

Anchors
- app/up.rs; src/cli.rs (flags/help)
- docs/BOOTSTRAP.md (Quick Start, Troubleshooting, examples)
- src/core/bootstrap.rs; src/core/status.rs; src/core/reporter.rs (events/logs)

Notes
- Leave implementation open for `--plan` rendering; keep JSON schema unchanged; prefer per-VM concurrency with responsive UI.
---

---

