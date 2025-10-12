Snapshot ref bump: v0.7.2. Output contract locked.

- Columns remain identical between default and --active modes; only STATUS content varies (Declared vs Active/Inactive with reasons if available).
- Performance guard: target <200ms for small projects; degrade gracefully with a note if backend inspection is unavailable.
- Help text: add `--active` flag description and stability note for scripting.


---

