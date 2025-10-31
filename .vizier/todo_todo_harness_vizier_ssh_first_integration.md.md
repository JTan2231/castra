Update — Evidence and next steps

Evidence:
- castra-harness/src/prompt.rs added with PromptBuilder and VmEndpoint; exposed via castra-harness/src/lib.rs. Provides a “vizier operational context” preamble suitable for session start.

Next steps (product-level):
- Incorporate the prompt preamble into the harness session open sequence, preceding the version preamble. Acceptance: consumers see context lines immediately after session start.
- Wire unified event stream: map core lifecycle events + vizier.ssh.* into a single ordered stream. Acceptance: golden test demonstrates merged ordering for at least one VM with interleaved stdout/stderr frames.
- Failure surfacing: include remediation_hint on vizier.ssh.failed and ensure clean disconnect events. Acceptance: retry of session after failure produces no duplicate connections.

Anchors:
- castra-harness/src/{session.rs, stream.rs, events.rs} for session open and stream composition.
- castra-core/src/app/up.rs for plan emission used by the vizier to establish SSH.


---

