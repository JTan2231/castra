Policy update: Ephemerality is mandatory; no built-in persistence/export

- Remove the "persistent mode" option entirely from scope for now. All VMs run with an ephemeral write layer that is always discarded on shutdown (graceful or forced). There is no built-in path to retain guest disk changes.
- If users need to preserve artifacts, they must export them over SSH (user-managed). We provide no built-in snapshot/export/archive features at this stage.

Behavioral requirements
- Always use ephemeral storage for launched VMs; teardown must remove all ephemeral layers and temp dirs deterministically after exit.
- On crash/reboot/orphan detection, perform bounded, opportunistic cleanup on the next command without blocking healthy operations.
- Any config/CLI flag attempting to enable persistence should be rejected with a clear error that ephemerality is currently the only supported mode, including remediation guidance: "Re-run and export data via SSH from within the guest before shutdown."
- Surface a concise TTY/JSON notice during up that the VM is ephemeral-only and changes will be discarded on shutdown, including a hint about SSH export.
- CLEAN continues to report reclaimed bytes from ephemeral layers and links managed-image evidence where available.

Acceptance criteria
- After shutdown, rerunning up starts from a pristine base image with no residual guest changes.
- No CLI options, env vars, or config keys permit persistent disks; attempts are validated and produce actionable errors.
- After normal shutdown, no ephemeral artifacts remain; after abnormal termination, the next command logs and reclaims orphans within bounded time.
- Up output (TTY and JSON) clearly indicates ephemeral-only behavior and suggests SSH export for data preservation; documentation/help reflect the same.

Cross-links
- Thread 12 (stamp-free bootstrap) remains compatible: host evidence is logs/events only; no host-side persistence of guest state.
- CLEAN integrates ephemeral layer reclamation as before.


---

