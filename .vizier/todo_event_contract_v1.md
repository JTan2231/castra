# Event Contract v1 (public, semver-governed)

Goal: Define and freeze the minimal JSON event shapes the UI relies on for Up/Down/Status/Clean/Bootstrap.

Why: Enables composability and decouples UI from core internals.

Acceptance criteria:
- Documented schema (fields, types, stability notes) with examples for each event class (progress, success, warning, error, remediation hint, summary types like BootstrapSummary).
- Version header/field advertised in stream start or per-event; semver policy documented in docs.
- Golden tests enforce the schema; changes require bump and test updates.

Scope:
- Cover per-VM lifecycle events, aggregate summaries, and diagnostics.
- Do not alter existing meanings without a version bump. Maintain backward compatibility where possible.

Anchors: castra-core/src/core/events.rs; castra-core/src/core/reporter.rs; castra-core/src/app/*; docs/.

Threads: Thread 20; interacts with Threads 2, 12, 13.