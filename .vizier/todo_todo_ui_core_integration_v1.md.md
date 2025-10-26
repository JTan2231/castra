
---
Update (blocking issue uncovered in CORE.md)

Observed gap: Broker launch path in castra-core reuses `current_exe()` and re-executes the embedding binary (`castra-ui`) with `broker`, creating a duplicate UI window and breaking composability.

Acceptance refinement:
- UI integration must proceed only after castra-core exposes a broker launch abstraction decoupled from `current_exe()`. UI will call into core/library mode and must not spawn any additional UI windows.

Verification additions:
- In the UI vertical slice demo, `pgrep -fal castra-ui` must not show a second UI process during `/up`.
- Add a smoke test in examples: library Up with a no-op broker runner; assert event flow works end-to-end.

Anchors: CORE.md; castra-core/src/core/runtime.rs; castra-core/src/core/broker.rs; castra-ui/src/controller/command.rs.

---

