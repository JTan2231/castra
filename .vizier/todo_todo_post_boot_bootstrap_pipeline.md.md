---
Additions (UX/plan mode acceptance)

Acceptance Criteria
- `--plan` dry-run prints, per VM, the resolved script path, payload dir (or none), SSH target/port inference, remote_dir, env deltas, and verify mode without side effects; exits non-zero if configuration is invalid.
- TTY mode shows compact progress lines per step (WaitHandshake, Connect, Transfer, Apply, Verify) and a final summary per VM including Success|NoOp|Skipped and a durable log path.
- JSON output remains schema-stable; only values change. Durable per-VM logs continue to be written under logs/bootstrap/<vm>.jsonl.
- Conflicting global/per-VM bootstrap settings are rejected preflight with actionable messages; per-VM overrides win when not in conflict.

Docs/Help
- CLI help and docs/BOOTSTRAP.md show `auto|always|skip` behaviors with examples and explicitly state stamp-free rerun behavior and `Castra:noop` sentinel usage.
---

---

