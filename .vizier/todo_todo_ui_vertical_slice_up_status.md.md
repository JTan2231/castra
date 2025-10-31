Alignment — harness preamble handling

Scope addition:
- UI must tolerate and render (or quietly skip) the version preamble followed by the vizier operational context preamble from the harness before standard events begin.

Acceptance addition:
- Manual demo and test confirm the UI does not misclassify the preamble lines as VM output and that timers/roster initialization waits until first lifecycle/vm event.

Anchors:
- castra-ui/src/controller/command.rs (stream subscription start), castra-ui/src/components/message_log.rs (preamble handling).

---

