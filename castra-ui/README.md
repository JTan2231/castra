# castra-ui

Castra’s GPUI-based front-end delivers a high-contrast control room for orchestrating VM fleets. It consumes the public Event Contract emitted by `castra-core` and renders roster, VM, and log views tuned for day-one productivity.

## Running the UI
```bash
cargo run -p castra-ui
```
The window launches with the prompt focused. Use `Ctrl+B`/`Cmd+B` to toggle the roster sidebar and `/help` to list available commands.

## Onboarding docs
- [Layout overview](docs/overview.md) — tour of the main surfaces and interaction model.  
- Component guides: [Roster](docs/components/roster_sidebar.md), [VM Fleet](docs/components/vm_fleet.md), [Message Log](docs/components/message_log.md), [Status Footer](docs/components/status_footer.md), [Prompt Shell](docs/components/prompt_shell.md).  
- [First `/up` tutorial](docs/tutorials/first_up.md) — step-by-step walkthrough using the minimal bootstrap example.  
- [Developer note: consuming Event Contract v1](docs/dev/consuming_event_contract.md).

Each page lists the assumed contract and attention-model versions and links to related design work (Threads 20–22).
