Progress sync — Snapshot v0.7.8

- Host CLI confirmed shipped (`bus publish`, `bus tail`); reuse of log follower UX validated. Next focus remains broker-side subscribe/ack/heartbeat/back-pressure and session timeout/cleanup.
- Add acceptance: status exposes per-VM bus freshness (`last_publish_age_ms`, `bus_subscribed`) without blocking; `bus tail` reflects subscription state via copy in log lines.
- Safety acceptance refined: disconnects deterministically clear subscription state; bounded retries on back-pressure before clean disconnect with reasoned log.

---

