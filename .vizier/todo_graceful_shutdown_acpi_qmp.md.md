Status note (Snapshot v0.7.5)
- No delivery yet; CLI copy references an attempted graceful shutdown. Keep product behavior unchanged until events+timeouts are wired.

Acceptance refinement
- Add explicit configurable timeouts (graceful, term, kill) in options/help; tests must assert ordered Events and per-VM independence under mixed responsiveness.

---

