Title: Remove/deprecate VM selection surfaces in CLI and Core

Context
- Runtime VM-choicing creates complexity and conflicts with agent-first addressing. We will remove these surfaces or deprecate with guidance.

Intent
- Simplify CLI/Core by eliminating VM selection flags/options and adjusting defaults to agent-scoped behavior via the harness.

Product scope and acceptance
- CLI help/manpages: no flags for choosing target VMs. If present today, they error with deprecation guidance and no side effects.
- Core options.rs/common.rs: remove VM selector parameters from public APIs. Operations derive scope from agent context provided by harness session.
- Tests updated: no tests rely on per-VM selection routing; CI green.
- Docs updated to reflect removal and agent-first flow.

Anchors
- castra-core/src/core/options.rs; castra-core/src/app/common.rs
- castra-core/src/app/{up,down,status,clean}.rs
- castra-core/tests
- castra/README.md, castra-core/docs

Thread links
- Serves Thread 31; coordinated with Thread 30 to avoid reintroducing bus-era semantics.Snapshot v0.10.0-pre update
- Alignment with agent-first model.
- Acceptance:
  - Replace VM-centric selectors with agent-centric affordances in CLI help and UI surface mapping; maintain compatibility shims where necessary.
  - Document interim mapping (VM → Agent) in README.
- Anchors: castra-core/src/cli.rs, castra-ui/src/components/roster_sidebar.rs.

---

