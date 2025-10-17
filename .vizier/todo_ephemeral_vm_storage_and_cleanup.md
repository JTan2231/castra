Title: Ephemeral VM storage and automatic cleanup (Thread 13 — Stateless runs)

Problem
- VMs provisioned for bootstrap currently risk leaving residual on-host state (mutable disks, scratch dirs) after shutdown. For bootstrap correctness and reproducibility, all guest-side mutations must vanish when the VM stops unless persistence is explicitly requested.

Product scope
1) Ephemeral write layer by default
- When launching a VM for `up` (including bootstrap flows), use a non-persistent write layer over the managed base image so that guest disk mutations are discarded on shutdown.
- Host artifacts that must persist (logs, events, stamps keyed by (base_image_hash, bootstrap_artifact_hash)) remain durable under the state root; guest disk contents revert to base on next run.

2) Deterministic cleanup on VM termination
- On normal cooperative shutdown and on forced termination, delete the ephemeral write layer and any per-VM temp directories immediately after the VM exits.
- On process crash or host reboot, recover and clean orphaned ephemeral layers on next `status`, `clean`, or `up` invocation without blocking healthy operations.

3) Policy enforcement, with explicit guidance
- Ephemeral (stateless) is the only supported mode. Reject attempts to opt into persistence via CLI/config with actionable guidance that points to SSH export workflows.
- Persistence policy must be reflected in help text and diagnostics; stamp semantics remain host-side and independent of guest disk state.

4) UX and evidence
- Render a one-line notice in non-JSON TTY output when ephemeral cleanup succeeds (e.g., “vmA: ephemeral changes discarded”).
- CLEAN command reports bytes reclaimed from ephemeral layers, linked to the VM instance/session, without requiring managed-image evidence.
- JSON mode: add stable fields to existing events only if already planned; otherwise, keep schema unchanged and rely on logs/diagnostics.

Acceptance criteria
- Start a VM, create files within the guest, shut it down (graceful or forced), start it again from the same base/stamp: created files are absent; base image remains unchanged.
- After shutdown, no per-VM ephemeral disk artifacts remain under the state root; CLEAN surfaces reclaimed bytes when applicable.
- Abnormal termination (kill -9 castra, host reboot) leaves at most bounded orphaned artifacts that are detected and cleaned on next command; operations remain responsive while cleanup proceeds.
- CLI rejects attempts to enable persistence and guides users toward SSH export prior to shutdown.
- Behavior is per-VM and concurrent-safe; does not alter existing JSON schemas for lifecycle or bootstrap events.

Pointers (anchors only)
- src/core/operations/up.rs and app/up.rs (launch surfaces)
- src/core/operations/clean.rs and app/clean.rs (reclamation)
- src/core/runtime.rs and app/down.rs (shutdown/teardown hook ordering)
- docs/BOOTSTRAP.md, CLEAN.md, and CLI help (docs + discoverability)

Notes
- Keep implementation open; acceptable approaches include copy-on-write overlays or transient snapshots managed by the hypervisor. Ensure durability of host-side stamps/logs remains untouched.
