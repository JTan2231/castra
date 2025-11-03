Thread: 30 — Busless architecture cutover (Core)

Why (tension): Current core depends on a local TCP broker/bus for VM/host messaging, but the product direction eliminates the shared comms substrate. The Codex harness (vizier) will manage VMs directly over SSH; the broker/bus creates unnecessary processes, logs, and coupling.

Desired behavior (product-level):
- Running any core operation (Up/Down/Status/Clean/Bootstrap) does not start or require a broker/bus. No bus handshake dir or bus log files are created.
- The library API and CLI do not expose or imply a bus requirement. “Broker” and “Bus” concepts are absent from help and user-facing errors.
- Legacy bus commands are deprecated with clear guidance (see separate CLI cleanup TODO) and cause no side effects.

Acceptance criteria (observable):
- From CLI: `castra broker` and `castra bus *` either disappear (command not found) or print a deprecation message and exit 0 without creating files or processes. No pidfile/logfile/handshake dirs appear.
- From library callers: operations::up_with_launcher runs without invoking any broker launcher; no env vars or options for broker are referenced in logs or diagnostics.
- Test sweep: all tests pass without creating `logs/bus/*` or `handshakes/*`. Any references to broker ports or greetings are removed.
- Docs no longer mention broker/bus as required components.

Notes and anchors (pointer-level):
- Remove/neutralize: castra-core/src/core/operations/bus.rs; castra-core/src/core/broker.rs; castra-core/src/app/broker.rs; castra-core/tests/broker_contract.rs; scripts/castra-bus-*.sh.
- Verify ops surfaces: castra-core/src/app/{up,down,status,clean,bootstrap}.rs; castra-core/src/core/runtime.rs.

Out of scope:
- SSH/vizier event wiring (covered by harness_vizier_ssh_first_integration).

Risk/Trade space:
- Breaking change for any external automation using bus shell scripts. Provide migration note and suggest harness vizier stream as replacement.Snapshot v0.10.0-pre update
- Evidence: broker/bus types and scripts remain; UI/harness moving busless.
- Acceptance (initial cut):
  - Remove broker handshake and contract artifacts from core schema/options while keeping CLI behavior stable.
  - Introduce deprecation notices where removals are not yet feasible; add migration doc pointers.
- Anchors: castra-core/src/core/options.rs, castra-core/src/core/broker.rs, scripts/.
- Risk: accidental breakage of examples; add CI check that examples still build and run.

---

Cutover steps (de-duped with CLI cleanup):
- Remove broker-era test: delete castra-core/tests/broker_contract.rs once harness golden tests land.
- Scripts: retire scripts/castra-bus-*.sh with deprecation notes in docs; ensure running them prints a clear deprecation message or is removed from distribution.
- Docs: add migration pointers to VIZIER_REMOTE_PROTOCOL.md and harness vizier stream.


---

