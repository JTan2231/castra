Alignment — include vizier operational context preamble

Adjustment:
- Golden streams must include the version preamble followed by the vizier operational context preamble (from PromptBuilder) before regular events.

Acceptance addition:
- Tests assert the presence and ordering of: version preamble -> vizier context header lines -> first core lifecycle/vizier.ssh event.

Anchors:
- castra-harness/src/prompt.rs for rendering; harness test harness to simulate session open sequence.

---

