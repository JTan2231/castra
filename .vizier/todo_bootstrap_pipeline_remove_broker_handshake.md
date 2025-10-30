Thread: 30 — Busless architecture cutover (Core)

Why (tension): The bootstrap pipeline currently waits on a broker-produced handshake file before attempting SSH. With the broker/bus removed, this step is both incorrect and a source of confusion.

Desired behavior (product-level):
- Bootstrap no longer depends on a broker handshake artifact. The first phase is SSH connectivity probing to the VM (using resolved host/port/user/identity/options).
- Event sequence removes `wait-handshake` or reinterprets it as `connect` without any file-based dependency.
- Timeouts/error messages reference SSH reachability rather than broker freshness windows.

Acceptance criteria (observable):
- Running `castra up` with a valid SSH forward proceeds directly to connectivity checks and subsequent steps (transfer/apply/verify). No `handshakes/` directory is created under the state root.
- Event stream no longer emits any message about broker or handshake freshness; the first bootstrap step is `connect` and succeeds/fails accordingly.
- Tests updated: remove handshake file writers/readers; success and failure cases assert the new sequencing and messaging.

Anchors (pointer-level):
- castra-core/src/core/bootstrap.rs (remove wait_for_handshake and related artifacts); castra-core/src/core/events.rs (update BootstrapStepKind, docs); castra-core/src/core/status.rs (if it references handshake freshness).