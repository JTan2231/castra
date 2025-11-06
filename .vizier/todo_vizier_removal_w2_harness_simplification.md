Thread 50 — Vizier Removal — Workstream 2: Harness simplification

Tension
- Harness currently proxies vizier.remote streams and maintains reconnection logic; pivot requires harness to become a metadata provider, not an I/O proxy.

Desired behavior (product level)
- Harness exposes metadata APIs for UI discovery (VM SSH info, context for prompts), with no vizier.remote protocol emission.
- No Vizier-specific config or modules remain.

Acceptance criteria
- `castra-harness` compiles and tests pass with `vizier_remote` removed.
- Public harness API contains discovery/metadata helpers; UI builds against them.
- Harness docs contain no Vizier references (except legacy notes).

Pointers
- castra-harness/src/vizier_remote/* (delete)
- castra-harness/src/{runner.rs,session.rs,events.rs,config.rs} (remove vizier types; add metadata)
- castra-harness/tests/vizier_remote.rs (delete or replace)

Notes
- Maintain behavioral parity for any non-Vizier telemetry promised to users.