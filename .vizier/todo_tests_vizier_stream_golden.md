Thread: 20 — Harness vizier stream goldens

Goal: Replace broker/bus golden tests with harness vizier unified stream tests.

Acceptance criteria:
- New golden tests capture the unified event stream (core + vizier.ssh) for the minimal/bootstrap examples. Tests assert version preamble and schema invariants.
- Remove castra-core/tests/broker_contract.rs. CI has no references to broker artifacts.
- Round-trip serde tests ensure fields required by consumers are present and stable.

Anchors:
- Where to place: castra-harness/tests/ or castra-harness/src/tests/*. Use whichever is standard in the repo.
- Example inputs: castra-core/examples/minimal-bootstrap, bootstrap-quickstart.