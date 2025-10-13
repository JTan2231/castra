Snapshot v0.7.4 update — Fields shipped in commit 1583c8c

Status
- `status` now exposes `reachable` and `last_handshake_age_ms` with stable names; CLI legend updated. This unblocks external automation and bootstrap triggers.

Remaining scope (product-level)
- Guarantee non-blocking behavior post-first observation and cap any initial probe to a documented bound; reflect this in tests.
- Emit clear handshake Events/log lines (start/success/stale) with VM identity for host logs; ensure deterministic prefixes for tooling.
- Document field semantics in `--json` help and README snippets that show polling at ~2s cadence. Maintain stability across minor releases.

Acceptance updates
- Repeated `castra status --json` calls show monotonically increasing `last_handshake_age_ms` without blocking; turning off the guest agent causes `reachable=false` after freshness window expiry.
- Host logs include a recognizable, single-line handshake success per VM per session.
- Help/legend text includes the freshness window description.

---

