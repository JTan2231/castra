# Bootstrap Pipeline Reference

Castra's post-boot bootstrap pipeline applies host-provided configuration (for example Nix flakes or shell scripts) once a VM becomes reachable via the in-guest Vizier service. The harness establishes SSH, verifies the Vizier handshake, and then drives host-provided scripts. This document describes how to steer the pipeline at invocation time and how to consume the structured events and logs it emits.

## Invocation Modes and Overrides

Each VM declares a bootstrap mode in `castra.toml` (`auto`, `skip`, or `always`). The CLI can override these settings per invocation:

```text
castra up --bootstrap skip              # skip bootstrap for all VMs
castra up --bootstrap always            # force bootstrap for every VM
castra up --bootstrap web=always        # override a single VM
castra up --bootstrap web=skip,db=always # CSV overrides targeting multiple VMs
```

Rules:

- Passing `--bootstrap <mode>` sets a global override (`auto`, `skip`, `always`).
- Passing `--bootstrap <vm>=<mode>` targets a specific VM by its expanded name (`api-0`, `web`, etc.).
- Multiple overrides are allowed; per-VM values take precedence over the global override.
- Unknown VM names cause a preflight failure so automation can surface configuration drift immediately.

## Event Stream Contract

The runtime exposes bootstrap progress through the `Event` stream shared by CLI output, reporters, and the JSON API. Events are emitted in a stable order per VM:

1. `BootstrapPlanned { vm, mode, action, reason, trigger?, script_path?, payload_path?, payload_bytes?, handshake_timeout_secs?, remote_dir?, ssh?, env_keys, verify?, artifact_hash?, metadata_path?, warnings[] }`
2. `BootstrapStarted { vm, base_hash, artifact_hash, trigger }`
3. `BootstrapStep { vm, step, status, duration_ms, detail? }` for each logical stage
4. `BootstrapCompleted { vm, status, duration_ms, stamp? }` *or* `BootstrapFailed { vm, duration_ms, error }`

Field reference:

| Event | Fields | Notes |
| --- | --- | --- |
| `BootstrapPlanned` | `vm: String`, `mode: BootstrapMode`, `action: BootstrapPlanAction`, `reason: String`, `trigger: Option<BootstrapTrigger>`, `script_path: Option<PathBuf>`, `payload_path: Option<PathBuf>`, `payload_bytes: Option<u64>`, `handshake_timeout_secs: Option<u64>`, `remote_dir: Option<String>`, `ssh: Option<BootstrapPlanSsh>`, `env_keys: Vec<String>`, `verify: Option<BootstrapPlanVerify>`, `artifact_hash: Option<String>`, `metadata_path: Option<PathBuf>`, `warnings: Vec<String>`, `vizier_status: Option<VizierPlanStatus>`, `vizier_log_path: Option<PathBuf>`, `vizier_remediation: Option<String>` | Dry-run summary emitted immediately before execution. `ssh` carries the resolved `ssh` command (user, host, port, options, identity) that the harness uses to form tunnels. Vizier planning fields highlight expected service health and log locations. |
| `BootstrapStarted` | `vm: String`, `base_hash: String`, `artifact_hash: String`, `trigger: BootstrapTrigger` | `trigger` is `auto` or `always`, mirroring mode resolution after overrides. |
| `BootstrapStep` | `vm: String`, `step: BootstrapStepKind`, `status: BootstrapStepStatus`, `duration_ms: u64`, `detail: Option<String>` | `step` values: `wait-handshake`, `connect`, `transfer`, `apply`, `verify`, `vizier-install`, `vizier-enable`, `vizier-handshake`. `status` is `success`, `skipped`, or `failed`. |
| `BootstrapCompleted` | `vm: String`, `status: BootstrapStatus`, `duration_ms: u64`, `stamp: Option<String>` | `status` is `Success` when work executed, `NoOp` when the bootstrap runner declares no changes. `stamp` is retained for schema stability and is currently always `null`. |
| `BootstrapFailed` | `vm: String`, `duration_ms: u64`, `error: String` | Emitted once per VM when the pipeline aborts; a durable log is written alongside the event. |

### JSON Example

Reporter consumers receive JSON like the following (abbreviated for clarity):

```json
{
  "BootstrapStarted": {
    "vm": "web-0",
    "base_hash": "3b2d…",
    "artifact_hash": "d71c…",
    "trigger": "auto"
  }
}
```

Step and completion events follow with matching `vm` identifiers; automation can correlate `duration_ms` to construct timelines.

## Durable Run Logs

Every bootstrap run appends a JSON log under `logs/bootstrap/` in the project state root. Filenames are `vm-timestamp.json` and capture the final disposition and step history:

```json
{
  "vm": "web-0",
  "artifact_hash": "d71cd6…",
  "base_hash": "3b2d8c…",
  "status": "success",
  "duration_ms": 8421,
  "steps": [
    { "step": "wait-handshake", "status": "success", "duration_ms": 420 },
    { "step": "vizier-install", "status": "success", "duration_ms": 900 },
    { "step": "vizier-enable", "status": "success", "duration_ms": 350 },
    { "step": "vizier-handshake", "status": "success", "duration_ms": 280 },
    { "step": "connect", "status": "success", "duration_ms": 1100 },
    { "step": "transfer", "status": "success", "duration_ms": 2100 },
    { "step": "apply", "status": "success", "duration_ms": 3800 },
    { "step": "verify", "status": "success", "duration_ms": 821 }
  ]
}
```

Failure logs retain the same envelope with `status: "failed"` and append a terminal step record:

```json
{ "step": "error", "status": "failed", "duration_ms": 0, "detail": "ssh exited with code 255" }
```

Castra does not persist host-side idempotence stamps; every invocation records a fresh log. If the bootstrap runner can detect a no-op, it reports that outcome through its own messaging while the host log continues to reflect the full pipeline execution. Vizier service logs are written to `state/vizier/<vm>/service.log` and should be consulted when bootstrap steps report Vizier failures.

## Consuming the Data

- Use the event stream for live progress. Each VM emits events independently and in order, making it safe to multiplex multiple machines in a single reporter.
- Tail the per-VM log directory for durable audit trails or to collect metrics after the run. The log schema is stable across retries and compatible with JSON tooling.
- When scripting `castra up`, use `--bootstrap` overrides to force or skip runs explicitly. Castra always attempts bootstrap on warm runs; runners may self-report no-op outcomes.
