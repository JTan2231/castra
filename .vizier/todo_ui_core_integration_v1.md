# Thread 20 — UI ↔ Core integration (Composable first)

Goal: Wire castra-ui to castra-core using the existing JSON event stream while keeping castra-core fully usable as a standalone library.

Why: We need a visible, low-risk path to UI value without coupling to internal core types. This de-risks iteration and preserves composability.

Acceptance criteria:
- Event Contract v1 is documented and versioned; UI only depends on this contract.
- castra-core can run up/down/status/clean/bootstrap as a library and emit the same events to an external subscriber; examples demonstrate usage without the UI.
- castra-ui can initiate an Up operation and render live per-VM progress from events (roster, footer, message log) using only the contract.

Scope (product-level):
- Choose transport boundary for the first slice: process boundary via existing JSON stream is acceptable; library subscription also supported/documented.
- Provide a sample workspace to validate the end-to-end flow.

Anchors: castra-core/src/core/events.rs; castra-core/src/app/*; castra-ui/src/controller/*; castra-ui/src/components/*; docs/AGENTS.md.

Tests/verification:
- Golden tests for event contract JSON.
- Example binaries/docs showing library usage to run Up and print events.
- Manual demo: start UI, launch Up, observe live updates and completion banner.

Threads: advances Thread 20; consumes Threads 2, 12, 13 outputs.