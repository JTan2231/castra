Title: UI Component Tutorials & Onboarding Docs (v1)

Threads: T20 (UI ↔ Core integration), T21 (Operational clarity)

Problem
New users and devs lack a concise, trustworthy guide to what the UI shows, how to interact with it, and how UI elements map to core events. This creates onboarding friction and inconsistent mental models.

Outcome (Product-level)
- A small, coherent set of markdown tutorials introduces the UI’s main surfaces and interaction model for both operators and developers.
- Each tutorial ties visuals to the Event Contract v1 and the attention model, so readers can predict behavior.
- Docs live alongside the UI (discoverable from castra-ui README) and stay stable with semver notes.

Acceptance criteria
- Overview doc explains the UI layout: Roster, VM Fleet, Message Log, Prompt/Shell, Status Footer; shows where to find active operations and health at a glance.
- Component guides (one page each) describe: what the component shows, key interactions (keyboard/mouse), and how it maps to event classes/attention levels. Includes at least one end-to-end example using the minimal bootstrap example.
- A “Run your first Up” tutorial that:
  - Uses examples/minimal-bootstrap to launch `up` via the UI controller.
  - Shows how per-VM progress appears, where errors/warnings surface, and how to read remediation hints.
  - Explains ephemeral vs persistent runs as surfaced in the UI.
- Developer note: Where the UI consumes Event Contract v1 and where to look in code (anchors only), without importing private core structs.
- Cross-links to: Event Contract v1 doc, Attention Model doc, and UI Vertical Slice (Up) doc.
- Versioning: Each page states the contract version(s) it assumes; breaking changes checklist included.

Scope and anchors (for orientation)
- castra-ui/README.md (entry point)
- New docs directory (e.g., castra-ui/docs/) with:
  - overview.md (layout + interaction model)
  - components/roster_sidebar.md
  - components/vm_fleet.md
  - components/message_log.md
  - components/status_footer.md
  - components/prompt_shell.md
  - tutorials/first_up.md
  - dev/consuming_event_contract.md
- Code anchors: castra-ui/src/components/*, castra-ui/src/controller/*, castra-core/src/core/events.rs, examples/minimal-bootstrap

Non-goals (v1)
- Full API reference or internal architecture deep-dive.
- Prescribing styling or theming details.

Quality bar
- Clear screenshots or text-based callouts showing where to look (images optional but helpful); commands and outputs are copy/pasteable.
- Each page has a small “What changed since last version” section tied to Event Contract/attention model versions.

Validation
- A new developer can follow the docs to run the minimal bootstrap, observe UI states during Up, identify an error state in Message Log, and locate remediation hints without asking for help.

Dependencies
- Event Contract v1 doc exists (T20) or is sufficiently stubbed.
- Attention Model doc exists (T21) or is sufficiently stubbed.
Pivot alignment (Vizier removal):
- Docs should describe mapping between UI components and agent sessions + harness metadata, not vizier.remote events.
- Update cross-links to reference agent session manager docs (castra-ui state/controller) and remove any vizier references.
- First Up tutorial: call out how agent session connects, where to see connection state and retries.

Validation note updated accordingly.

---

