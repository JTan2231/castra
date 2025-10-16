
---
UX polish — make bootstrap user friendly (thread: post-boot-bootstrap-pipeline)

Problem
- Functionally solid but confusing to operate: flags are hard to discover, outputs are verbose without clear summaries, and remediation for common failures (ChannelUnavailable/Handshake timeout) is not obvious.

Scope (product level)
1) Simplify/clarify CLI affordances
   - Normalize modes to a single, discoverable form: `--bootstrap=auto|always|skip` with optional per‑VM overrides `--bootstrap vmA=skip,vmB=always`.
   - Help text includes concise examples for single VM, multiple VMs, and JSON streaming use.
   - `--plan` dry‑run prints what would bootstrap and why (Success|NoOp|Skipped rationale) without side effects.

2) Human‑friendly progress and summaries
   - During `up`, render a compact per‑VM progress line (Waiting for handshake → Running steps → Completed/NoOp/Skipped/Failed) that coexists with JSON mode unchanged.
   - At completion, print a per‑VM one‑line outcome with total duration and next‑step hint on failure.

3) Actionable errors and remediation hints
   - When bootstrap fails due to handshake timeout or channel issues, show a short hint (e.g., “Guest never reached broker — verify network and broker service status”) with a pointer to durable log path.
   - Map common failures to 3–5 friendly hints; do not suppress underlying detail.

4) Discoverability and docs
   - BOOTSTRAP.md gets a “Quick Start” with copy‑paste examples; a “Troubleshooting” table mapping symptoms → causes → actions; and examples of NoOp vs Always vs Skip.
   - `--help` surfaces bootstrap section with examples and a link to docs.

Acceptance Criteria
- `castra up --bootstrap=auto` (default) behaves as today; `always` forces, `skip` disables; per‑VM CSV overrides work and are validated; conflicts error preflight with a clear message.
- `--plan` produces a deterministic summary per VM without side effects; return code 0 if plan computed, non‑zero if configuration invalid.
- Non‑JSON TTY output shows compact progress and final one‑liners; `--json` output remains unchanged and machine‑stable.
- On failure, users see a clear hint plus path to the durable log; hints cover at least handshake timeout, SSH auth failure, and missing artifact.
- Docs updated with Quick Start, Troubleshooting, and mode examples; `--help` includes examples and doc link.

Pointers
- app/up.rs (rendering and CLI arg parsing)
- cli.rs (help text)
- docs/BOOTSTRAP.md (Quick Start, Troubleshooting)
- app/error.rs or reporter surfaces for hint mapping; core/events.rs for stable JSON unaffected

Notes
- Keep implementation open: prefer adapter layers that translate existing events into friendlier TTY lines; avoid changing JSON schema. Ensure tests cover help, plan mode safety, and per‑VM override parsing.


---

