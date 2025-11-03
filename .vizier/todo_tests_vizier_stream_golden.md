Thread: 20 — Harness vizier stream goldens

Goal: Replace broker/bus golden tests with harness vizier unified stream tests.

Acceptance criteria:
- New golden tests capture the unified event stream (core + vizier.ssh) for the minimal/bootstrap examples. Tests assert version preamble and schema invariants.
- Remove castra-core/tests/broker_contract.rs. CI has no references to broker artifacts.
- Round-trip serde tests ensure fields required by consumers are present and stable.

Anchors:
- Where to place: castra-harness/tests/ or castra-harness/src/tests/*. Use whichever is standard in the repo.
- Example inputs: castra-core/examples/minimal-bootstrap, bootstrap-quickstart.Snapshot v0.10.0-pre update
- Tie golden tests to vm_commands.sh as acceptance harness.
- Required assertions:
  - First line/preamble contains version + session id; stable regex documented.
  - Event framing is stable across minor versions (documented invariants); unknown fields ignored by consumers.
  - Round-trip: stream → UI transcript writer preserves ordering; verify with sample transcript fixture.
- Artifacts: check fixtures into castra-harness/tests/ with README on regeneration policy.

---

Scope refine:
- Update assertions and fixtures to use vizier.remote.* family and preamble fields defined in VIZIER_REMOTE_PROTOCOL.md.
- Add reconnect scenario: drop SSH mid-stream via vm_commands.sh and assert reconnect events and ordering are preserved.
- Latency check: measure echo round-trip and assert ≤150ms under localhost lab conditions; skip or relax in CI if environment variance detected (document policy in fixture README).


---

