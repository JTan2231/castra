# Operational clarity: attention model and UX mapping

Goal: Turn core signals into a high-clarity, low-noise UI aligned with a video-game aesthetic (what needs attention pops; everything else fades).

Acceptance criteria:
- Document attention levels (info/progress/warn/error/blocker) and their visual treatments (badge color, pulse, sound optional, footer indicators).
- Map specific core events to levels and remediation hints (e.g., ChannelError → blocker with retry hint).
- Implement grouping rules and rate limits for repetitive events; ensure nothing critical is hidden by default.

Scope:
- Begin with Up and Bootstrap events; extendable to Down/Clean/Status.
- Keep visuals accessible and legible in TTY-like palettes.

Anchors: castra-ui/src/components/*; castra-core/src/core/events.rs; docs/AGENTS.md; castra-ui/AGENTS.md.

Threads: Thread 21.