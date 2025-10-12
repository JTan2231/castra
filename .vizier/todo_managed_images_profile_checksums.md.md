Snapshot ref bump: v0.7.2. Diagnostics copy and cache behavior clarified.

- On checksum mismatch, instruct user to remove the specific cached file path; do not delete automatically.
- On offline, state whether a valid cache exists (hit) and whether the operation can proceed from cache; if not, fail with an offline-specific message.
- Events: include "verified source checksums" before transform and "applied boot profile" when overrides are used.


---

