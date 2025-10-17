**Up & Launch**
- Trigger: `castra up` loads the project, applies CLI bootstrap overrides, and aborts if any VM is already running so operators do not trample live guests (src/core/operations/mod.rs:113-143).
- Execution path: host capacity, disk headroom, and port availability checks run before any process is spawned; `--force` converts hard failures into warnings when needed (src/core/operations/mod.rs:144-163).
- Runtime prep: the launcher provisions managed images/overlays, then emits structured events while starting the broker and each VM so both humans and automation see progress in order (src/core/operations/mod.rs:164-259, src/core/runtime.rs:294-360, src/core/runtime.rs:434-612, src/core/events.rs:11-211).
- Configuration influence: lifecycle wait windows, bootstrap overrides, and handshake defaults come straight from `castra.toml`, with CLI overrides rewriting those fields before launch (src/config.rs:97-175, src/core/operations/mod.rs:117-206).

**Bootstrap Pipeline**
- Trigger: once VMs launch, `bootstrap::run_all` fans out worker threads so each VM’s pipeline runs concurrently while feeding events back over a channel (src/core/bootstrap.rs:60-112).
- Execution path: every run waits for a fresh broker handshake, then steps through connect → transfer → apply → verify, emitting success/failure events and writing a durable JSON log under `logs/bootstrap/` (src/core/bootstrap.rs:228-512).
- Failure handling: missing scripts, handshake timeouts, or SSH/verification errors short-circuit the run, surface diagnostics, and persist failure logs for post-mortems (src/core/bootstrap.rs:152-336, src/core/bootstrap.rs:342-473).
- Observability: events encode step names, durations, triggers, and final status so CI or dashboards can reconstruct timelines without scraping stdout (src/core/events.rs:168-211).

**Status & Observability**
- Trigger: `castra status` rebuilds a reachability snapshot by inspecting pidfiles, broker state, and cached handshakes without blocking on live probes (src/core/operations/mod.rs:399-440, src/core/status.rs:35-129).
- Broker signals: the broker appends handshake and bus activity to `handshake-events.jsonl`/`bus-events.jsonl`, giving operators durable evidence of guest connectivity decisions (src/core/broker.rs:20-186).
- Logs: `castra logs` slices per-VM QEMU/serial output plus broker logs from the state root, with optional follow mode for streaming incident response (src/core/logs.rs:12-49).
- Freshness rules: reachability flips once a handshake ages past 45 s, aligning bootstrap waits and status tables on the same freshness window (src/core/status.rs:17-107, src/core/bootstrap.rs:239-276).

**Shutdown & Cleanup**
- Trigger: `castra down` spawns one thread per VM, streams ordered shutdown events, and only returns when every worker reports completion (src/core/operations/mod.rs:268-357).
- Execution path: each worker tries a cooperative ACPI/QMP shutdown, records timeouts or channel errors, then escalates with SIGTERM/SIGKILL while keeping pidfiles and sockets tidy (src/core/runtime.rs:668-979).
- Event order: operators see `ShutdownRequested → CooperativeAttempted → CooperativeSucceeded|CooperativeTimedOut → ShutdownEscalated → ShutdownComplete`, matching the acceptance contract in `.vizier` (src/core/events.rs:117-211, src/core/runtime.rs:683-977).
- Cleanup: `castra clean` refuses to run if VMs or the broker are live (unless `--force`), then reclaims managed images, logs, handshake archives, overlays, and pidfiles while reporting reclaimed bytes and managed-image evidence (src/core/operations/clean.rs:26-234, src/core/operations/clean.rs:314-360).

**Configuration Controls**
- Lifecycle knobs (graceful, TERM, KILL waits), bootstrap modes, handshake timeouts, and remote staging paths all default in config but can be overridden per run for different environments (src/config.rs:97-173, src/core/operations/mod.rs:278-288, src/core/bootstrap.rs:733-511).
