# Event Contract v1 (Placeholder)

**Status:** Draft scaffold published to support Thread 22 docs. Full schema lands with Thread 20 deliverable.  
**Updated:** 2024-05-21

## What changed since last version
- Initial placeholder describing event families and pointing to source files/tests.

## Scope
- Governs JSON events emitted by castra-core operations (`up`, `down`, `status`, `clean`, `bootstrap`).  
- Aligns severity with the Attention Model draft (info, progress, warn, error, blocker).  
- Designed for consumption by castra-ui and external automation.

## Event families
- **message** — Human-readable progress with severity. Source: `Event::Message` in `src/core/events.rs`.  
- **vm.lifecycle** — Overlay prep, launch, shutdown, cooperative attempts, escalations.  
- **bootstrap.* ** — Plan/start/step/completion/failure payloads describing the host+guest pipeline.  
- **broker** — Broker process start/stop telemetry.  
- **cleanup** — Artifact removal progress, including reclaimed bytes and dry-run flag.  
- **command** — Acceptance or rejection of user-issued commands (soon to be wired through `controller::command`).  
- **summary** — High-level run conclusions and aggregated metrics.

## Source of truth
- Enum definition: `castra-core/src/core/events.rs`.  
- Reporter wiring: `castra-core/src/core/reporter.rs`.  
- Tests (add soon): `castra-core/tests/` — goldens should lock JSON structure per semver policy.

## Embedding castra-core (Rust)
- Invoke operations via `castra::core::operations::up_with_launcher`, providing a `BrokerLauncher` implementation that suits your embedding.  
- The supplied `ProcessBrokerLauncher` wraps the existing CLI binary; pass its path explicitly rather than relying on `current_exe`.  
- See `cargo run --example library_up -- --plan --cli /path/to/castra` for a minimal reporter that prints events and diagnostics without launching VMs.

## Emitting JSON across process boundaries
- Implement `castra::core::reporter::Reporter` and translate each `Event` variant into your schema before forwarding across IPC.  
- Preserve the contract’s field names and severity levels when serialising so downstream consumers stay aligned.  
- A good pattern is to reuse the enums as tags (e.g. `{ "type": "vm.launch", ... }`) and attach the raw message text for logging/debugging.

## Next steps
- Document payload schemas, field types, and sample JSON in a future revision.  
- Add a “Breaking changes checklist” once Thread 20 finalizes version bump mechanics.

## Related docs
- [Attention Model draft](../../castra-ui/docs/reference/attention_model.md)  
- [UI Vertical Slice: Up](../../UP.md)
