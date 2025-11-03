Thread: 30 — Busless architecture cutover (Core)

Goal: Remove or deprecate CLI commands related to the broker/bus and update help/manpages accordingly.

Acceptance criteria (product-level):
- `castra broker` and `castra bus publish/tail` are no longer operational. Invocations return a concise deprecation message that points users to the Codex harness vizier stream or are removed entirely from the CLI.
- `castra --help` and subcommand helps contain no references to broker/bus. Docs in README and castra-core/docs reflect this change.
- Regression protection: a test ensures invoking these commands does not create any files or processes and exits with code 0 (if deprecation path) or signal a friendly error if removed.

Anchors (pointer-level):
- CLI wiring: castra-core/src/cli.rs; castra-core/src/app/broker.rs; castra-core/src/core/operations/bus.rs.

User guidance:
- Print: "Deprecated: bus/broker have been removed. Use the Codex harness (vizier) for per-VM SSH control and event streaming."CLI deprecation messaging:
- `castra broker` and `castra bus *` print: "Deprecated: bus/broker have been removed. Use the Codex harness vizier stream over SSH (see VIZIER_REMOTE_PROTOCOL.md)." and exit 0, until full removal.
- Help text scrub: ensure `castra --help` and subcommands contain no bus/broker mentions; link to the migration doc in castra-core/docs.


---

