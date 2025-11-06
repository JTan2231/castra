# Event Contract v1

**Status:** Stable as of Castra snapshot v0.10.0-pre  
**Updated:** 2024-06-02

## Overview
- Event Contract v1 describes the JSON payloads emitted by `castra::core` during long-running operations (`up`, `down`, `clean`, `status`) and by the Codex harness while relaying transcript updates and usage summaries.  
- Payloads are newline-delimited JSON objects. Each object carries a `type` field (e.g. `vm.lifecycle`, `bootstrap.step`, `command.accepted`) plus family-specific fields.  
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
- `bootstrap.planned` includes resolved SSH metadata (`ssh.user`, `ssh.host`, `ssh.port`, `ssh.identity`, `ssh.options`) and warning text that downstream consumers use to render direct session helpers.

### `usage`
- Codex harness usage summaries stream as `usage` events reporting prompt, cached, and completion token counts. Aggregate alongside bootstrap metadata to produce accurate footer totals.

### `cleanup`
- Reports artifact cleanup progress (path, category, bytes, dry-run flag) during `castra clean` and automatic overlay reclamation.

### `command`
- Captures CLI command accept/reject decisions. Variants: `command.accepted`, `command.rejected`, `command.completed`, `command Failed` (subject to future expansion).  
- Consumers should present `command.rejected.detail` directly to operators.

### `summary`
- Wrap-up statistics for an operation (duration, exit status, VM counts, bytes transferred).  
- Intended for dashboards and end-of-run notifications.

## Versioning And Compatibility
- Additive fields in event payloads must be feature-gated so older consumers safely ignore them. Removing or renaming existing fields requires bumping the contract version.  
- Consumers should treat unknown event types as no-ops to remain forward compatible.

## Tests And Tooling
- Include `rg '(broker|bus)'` in release validation to ensure only migration docs retain historical references.  
- When adding new event families or fields, update this document and the corresponding UI docs before merging.
