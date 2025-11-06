# Castra UI Layout Overview

**Assumed versions:** Castra snapshot v0.10.0-pre · Castra UI 0.1.0 · Event Contract v1 · Attention Model draft v0.1  
**Updated:** 2024-06-02

## What changed since last version
- Initial publication for Thread 22 onboarding docs set.

## Map of the interface
- **Roster sidebar** (optional, left edge) lists your agents and highlights the active routing target for prompt messages. Toggle visibility with `Ctrl+B` (`Cmd+B` on macOS).  
- **VM Fleet columns** sit on both sides of the message log. Each card reflects one VM’s lifecycle, using attention colors from the model (progress green, warning amber, error red).  
- **Message log** occupies the center pane, showing the ordered stream of Event Contract messages with timestamps and speakers (system, user, agents, operations).  
- **Prompt shell** anchors the bottom, accepting free text or slash commands. Submissions become `user.command` or `user.input` events consumed by castra-core.  
- **Status footer** spans the lower bar, summarizing active agent, clock, and shortcut hints. When wired to live data it also surfaces aggregate attention (next iteration per Thread 21).

## Interaction highlights
- Focus the prompt instantly with `Ctrl+L` (`Cmd+K` on macOS). Enter `/help` to list available commands; `/up` launches the default minimal bootstrap run described in the tutorial.  
- Switch agents with `Ctrl+1…9` (`Cmd+1…9` on macOS). The active agent label updates in the footer and future Event Contract routing follows that context.  
- Navigate command history with `↑`/`↓`. The prompt preserves a draft so you can peek at previous commands without losing in-progress edits.  
- Click a VM card to reveal richer details (planned expansion) or hover to read the last reported bootstrap or lifecycle message.  
- Scroll the message log; repeated system messages collapse according to the Attention Model grouping rules once Thread 21 lands.

## Event flow at a glance
- castra-core streams JSON events (e.g. `bootstrap.step`, `vm.state`, `operation.summary`) that map directly into UI render state.  
- Harness surfaces SSH metadata (host, port, identity hints, wrapper paths) for each VM. The UI and `vm_commands.sh` wrappers use that data to manage direct agent sessions; no in-guest steward is required.  
- Roster badges mirror `agent.status` updates; VM cards hydrate from `vm.lifecycle` and `bootstrap.*` events, including the `ephemeral` flag exposed by Event Contract v1.  
- Message log renders every `message` event with severity styling from the attention model, balancing signal vs noise.  
- Status footer presents aggregate `operation.progress` data (active counts, last attention bump) so operators know when to intervene.  
- Prompt submissions produce `command.requested` followed by `command.accepted` or `command.rejected` events; the UI acknowledges them inline.

## Further reading
- [Event Contract v1](../../castra-core/docs/event-contract-v1.md) — canonical schema and stability guarantees.  
- [Attention Model draft](reference/attention_model.md) — severity bands, grouping rules, and remediation hints.  
- [UI Vertical Slice: Up](../../UP.md) — execution plan for wiring `/up` end-to-end.
