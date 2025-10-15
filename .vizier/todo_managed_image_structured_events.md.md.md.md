

Update — clarify acceptance and stability requirements
- Events must be declared as explicit variants in src/core/events.rs to ensure type-stable emission across all reporters (no ad-hoc JSON-only events).
- Maintain durability: these events appear in the same unified channel as lifecycle events and persist in per-image logs.


---

