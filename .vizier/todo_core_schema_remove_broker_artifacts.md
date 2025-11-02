Thread: 30 — Busless architecture cutover (Core)

Goal: Remove broker/bus presence from core-facing types, options, and outcomes so that no public surface implies a broker.

Acceptance criteria (product-level):
- Config: ProjectConfig no longer contains a `[broker]` section or `broker.port`. Port conflict summaries no longer include broker-reserved warnings.
- CLI options: `--broker-only` removed from `castra up`. Help text and errors contain no broker references.
- Events: Event enum removes `BrokerStarted`/`BrokerStopped` variants.
- Outcomes: UpOutcome/DownOutcome/StatusOutcome no longer include broker fields. Status output does not report broker state. Clean/Logs no longer reference `handshakes/`.
- Public API: lib exports remove Broker* types and bus publish/tail operations.

Guard rails:
- Provide deprecation-friendly errors for legacy config fields (clear message and migration hint) until a major bump is finalized, or fail-fast with a targeted error if deprecation path is not desired.

Anchors (pointer-level):
- castra-core/src/core/{events.rs, operations/mod.rs, ports.rs, status.rs, runtime.rs}; castra-core/src/lib.rs; castra-core/src/app/{up.rs,*.rs}; castra-core/docs/*.

Notes:
- Coordinate with docs_bus_deprecation_and_migration and cli_cleanup_bus_commands.
- Expect substantial test rewrites and fixture updates.Snapshot v0.10.0-pre update
- Scope the first pass to schema/options and visible CLI flags only; avoid deep refactors.
- Acceptance:
  - No broker/bus types in options.rs public surfaces; any remaining internals clearly quarantined.
  - CLI help no longer mentions broker/bus; deprecation banner added to docs.
- Anchors: castra-core/src/core/options.rs, castra-core/src/cli.rs.

---

