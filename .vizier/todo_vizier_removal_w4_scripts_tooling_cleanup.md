Thread 50 — Vizier Removal — Workstream 4: Scripts & tooling cleanup

Tension
- Tooling references Vizier units/logs (`vm_commands.sh`, docs), which will be stale post-removal.

Desired behavior (product level)
- Shell tools expose only VM lifecycle and generic log retrieval affordances; no Vizier-specific commands.

Acceptance criteria
- `vm_commands.sh` contains no vizier-* commands; helper alternatives documented if necessary.
- Grep for "Vizier" in scripts/tooling returns none (except legacy notes).

Pointers
- vm_commands.sh (remove vizier commands)
- in-progress/, docs/, examples/ (scan and excise Vizier mentions)