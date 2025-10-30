Thread: 30 — Remove bus shell scripts

Goal: Eliminate bus-related shell scripts and their references.

Acceptance criteria:
- scripts/castra-bus-*.sh and castra-handshake.conf.example are removed or replaced with harness-focused tools (if any).
- scripts/README.md updated or pruned; no bus mention remains.
- Repo search shows no remaining references to these scripts.

Anchors:
- castra-core/scripts/*