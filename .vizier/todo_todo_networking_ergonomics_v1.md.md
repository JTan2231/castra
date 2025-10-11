Update (SNAPSHOT v0.4)

Evidence
- QEMU launched with user-mode networking and declared hostfwd entries; ports view shows declared forwards and flags conflicts, including broker-port overlap.
- macOS/Linux acceleration flags applied (HVF/KVM) and `-cpu host` used.

Refinement
- In `status`, distinguish active vs planned forwards once VM is running (requires inspecting QEMU args or QMP).
- Add a `castra ports --active` mode once runtime inspection exists.

Acceptance criteria (amended v0.4)
- v1 behavior delivered via NAT + hostfwd; conflicts flagged; broker port visible. [DONE]
- Active-forward inspection is a v1.1 enhancement. [NEXT]


---

