Status update — delivery and remaining work (Snapshot v0.7.5)
- Delivery: CLI now enforces that `--skip-discovery` requires `--config` for status/up/down/ports/logs; help text updated accordingly. Internal options path avoids any upward directory walking when flag is set.
- Remaining: add tests that assert no filesystem walking occurs and verify exit codes/copy; extend same semantics to future `clean` by allowing `--state-root` as the explicit path alternative.

Acceptance refinement
- Add integration tests covering: (1) `--skip-discovery` without `--config` → fails fast with guidance; (2) `--skip-discovery --config <path>` → zero directory probes; (3) parity for `clean` once introduced (`--skip-discovery` must pair with `--config` or `--state-root`).
- Ensure help text examples include both `--config` and, for `clean`, `--state-root` pairing.

Cross-links
- Thread 14 (clean): share the same explicitness contract and exit behavior.

---

