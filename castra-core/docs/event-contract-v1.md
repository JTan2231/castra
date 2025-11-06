# Event Contract v1

**Status:** Stable as of Castra snapshot v0.10.0-pre  
**Updated:** 2024-06-02

## Overview
- Event Contract v1 describes the JSON payloads emitted by `castra::core` during long-running operations (`up`, `down`, `clean`, `status`) and by the Codex harness while proxying Vizier tunnels.  
- Payloads are newline-delimited JSON objects. Each object carries a `type` field (e.g. `vm.lifecycle`, `bootstrap.step`, `vizier.remote.handshake`) plus family-specific fields.  
- Downstream consumers (Castra UI, automation, third-party dashboards) must treat field names and semantics as stable until the contract revs. Additive fields may appear with safe defaults; breaking changes trigger a new contract revision.

## Event Families

### `message`
- Textual progress with an Attention Model severity value (`info`, `progress`, `warn`, `error`, `blocker`).  
- Source: `Event::Message` in `castra-core/src/core/events.rs`.  
- Intended for operator logs, status toasts, and high-level dashboards.

### `vm.lifecycle`
- Lifecycle events for VM processes: overlay preparation, launch, shutdown requests, cooperative attempts, escalations, and completion.  
- Fields vary per variant (`vm`, `pid`, `timeout_ms`, `elapsed_ms`, etc.).  
- Source: `Event::OverlayPrepared` … `Event::ShutdownComplete`.

### `bootstrap.*`
- Describes the host-side bootstrap pipeline: planning (`bootstrap.planned`), execution (`bootstrap.started`, `bootstrap.step`), completion (`bootstrap.completed`), and failure (`bootstrap.failed`).  
- Step events expose `step`, `status`, `duration_ms`, and optional human-readable detail strings.  
- Vizier planning metadata (`vizier_status`, `vizier_log_path`, `vizier_remediation`) surfaces through the planned event.

### `vizier.remote.*`
- Harness-originated stream describing the SSH tunnel between the host and the in-guest Vizier.  
- Variants include:
  - `vizier.remote.handshake` — protocol negotiation success. Fields: `vm`, `protocol_version`, `vm_vizier_version`, optional `log_path`, and `capabilities` (echo latency hint, reconnect flag, system events flag).  
  - `vizier.remote.handshake_failed` — protocol or setup failure. Fields: `vm`, optional `protocol_version`, optional `vm_vizier_version`, `message`, and `remediation_hint` (usually pointing to `state/vizier/<vm>/service.log`).  
  - `vizier.remote.output` — stdout/stderr frames (`stream`, `message`).  
  - `vizier.remote.system` — structured Vizier service logs (`category`, `message`).  
  - `vizier.remote.status` — Vizier service state transitions (`status`, optional `detail`).  
  - `vizier.remote.control` — control plane notifications (`event`, optional `reason`).  
  - `vizier.remote.usage` — token accounting (`prompt_tokens`, `cached_tokens`, `completion_tokens`).  
  - `vizier.remote.ack` — acknowledgement of control frame IDs (`id`).  
  - `vizier.remote.reconnect_attempt` / `vizier.remote.reconnect_succeeded` / `vizier.remote.disconnected` — tunnel lifecycle signals with backoff metrics.  
  - `vizier.remote.error` — unexpected tunnel failures (`scope`, `message`, optional raw payload).

### `cleanup`
- Reports artifact cleanup progress (path, category, bytes, dry-run flag) during `castra clean` and automatic overlay reclamation.

### `command`
- Captures CLI command accept/reject decisions. Variants: `command.accepted`, `command.rejected`, `command.completed`, `command Failed` (subject to future expansion).  
- Consumers should present `command.rejected.detail` directly to operators.

### `summary`
- Wrap-up statistics for an operation (duration, exit status, VM counts, bytes transferred).  
- Intended for dashboards and end-of-run notifications.

## Vizier Remote Sample Payloads

```jsonc
{ "type": "vizier.remote.handshake",
  "vm": "alpha",
  "protocol_version": "1.0.0",
  "vm_vizier_version": "1.0.3",
  "log_path": "state/vizier/alpha/service.log",
  "capabilities": {
    "echo_latency_hint_ms": 42,
    "supports_reconnect": true,
    "supports_system_events": true
  }
}
{ "type": "vizier.remote.reconnect_attempt",
  "vm": "alpha",
  "attempt": 3,
  "wait_ms": 2000
}
{ "type": "vizier.remote.handshake_failed",
  "vm": "beta",
  "protocol_version": "0.9.0",
  "vm_vizier_version": "0.8.7",
  "message": "Vizier protocol 0.9.0 is below supported >=1.0.0, <2.0.0.",
  "remediation_hint": "Update vizier to a protocol within >=1.0.0, <2.0.0; inspect logs at state/vizier/beta/service.log."
}
```

## Versioning And Compatibility
- The harness validates `protocol_version` against the supported range advertised by `castra-protocol`. Unsupported versions trigger `vizier.remote.handshake_failed` and close the tunnel.  
- Additive fields in handshake capabilities must be feature-gated so older UIs safely ignore them. Removing or renaming existing fields requires bumping the contract version.  
- Consumers should treat unknown event types as no-ops to remain forward compatible.

## Tests And Tooling
- Golden transcripts for Vizier tunnels live under `castra-harness/tests/fixtures/vizier_remote/`.  
- `cargo test -p castra-harness vizier_remote` exercises handshakes, reconnects, and error scenarios.  
- Include `rg '(broker|bus)'` in release validation to ensure only migration docs retain historical references.  
- When adding new event families or fields, update this document and the corresponding UI docs before merging.

