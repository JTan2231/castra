Pivot — remove bus/broker families; add vizier.ssh + preamble

Scope adjustments:
- Explicitly exclude any broker/bus event families from v1. Introduce vizier.ssh.* events (connecting, connected, stdout, stderr, failed{remediation_hint}, disconnected).
- Require a version preamble at stream start, followed by an optional vizier operational context preamble (from harness PromptBuilder).

Acceptance additions:
- Examples include unified stream samples showing version + vizier context, then interleaved core lifecycle and vizier.ssh events for at least one VM.
- Golden tests relocated to harness (see Thread 20 tests). castra-core no longer ships broker contract tests.

Anchors:
- castra-harness/src/events.rs (event enums/mapping), castra-harness/src/prompt.rs (context preamble), castra-core/src/core/events.rs (core lifecycle).

---

