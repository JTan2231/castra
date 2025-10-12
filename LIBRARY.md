# Castra Library Enablement Plan

Castra should feel equally natural to drive from Rust code as from the current CLI. The long-term goal is a crate that exposes a small, ergonomic API surface, with the CLI acting as a specialised frontend layered on top. This document records the plan to get there without breaking existing behaviour.

## Goals

- Provide library entry points for every CLI workflow (`init`, `up`, `down`, `status`, `ports`, `logs`, hidden `broker`), returning structured data rather than printing directly.
- Keep the CLI UX unchanged: same stdout/stderr messaging, colours, progress, and exit codes.
- Preserve backwards compatibility during the transition by extracting shared code incrementally and keeping tests passing.

## Guiding Principles

- Keep core logic presentation-free. No `println!`/`eprintln!` inside public library functions; surface diagnostics as structured values or streamed events.
- Errors must be library-friendly. Rename `CliError` to a neutral `Error`, capture machine-consumable context, and provide a CLI-only adapter for exit-code mapping.
- The CLI becomes a thin adapter: parse args, call library APIs, render outcomes using existing formatting helpers.
- All steps land incrementally. Each stage should keep the CLI building and running; avoid multi-file rewrites that lack intermediate shims.

## Stage 0 – Baseline & Contracts

- Write an architecture memo summarising current control flow across `src/app`, `src/runtime`, `src/config`, and `src/managed`, and highlight where CLI types leak into business logic.
- Capture transcripts for each CLI command in a sample project to use as regression references for text output.
- Catalogue configuration structures and filesystem layout expectations (state dirs, overlays, broker files). Note which functions already return structured data (`collect_vm_status`) versus relying on printing side effects.
- Document lifecycle management for broker and VM processes, including how PID files are currently used.
- Audit existing tests (`cargo test`) and note coverage gaps relevant to the future library API.

## Stage 1 – Core Extraction

- Introduce `src/lib.rs` with a `core` module namespace and re-export the current shared structures (`ProjectConfig`, `VmDefinition`, etc.) without renaming to minimise churn.
- Move CLI-agnostic helpers (`collect_vm_status`, `prepare_runtime_context`, `load_or_default_project`) into the new `core` modules. Provide transitional `pub use` so `main.rs` continues compiling.
- Replace direct usage of Clap argument structs inside core logic with internal option structs (`InitOptions`, `UpOptions`, `StatusOptions`, …) that mirror existing flags. The CLI converts input into these options.
- Remove direct printing from core functions. Return structured diagnostics (e.g., `Vec<Diagnostic>` with severity enums) or stream events through a `Reporter` trait implemented by the CLI.
- Rename `CliError` to `Error`, keep variant coverage, and move exit-code mapping into a CLI-only adapter.
- Update runtime helpers (`check_host_capacity`, `check_disk_space`, etc.) to emit warnings via structured diagnostics rather than printing.

## Stage 2 – Library API Surface

- Publish public entry points for each operation: `init`, `up`, `down`, `status`, `ports`, `logs`, and `broker`. Each function accepts its option struct plus an optional reporter and returns an outcome struct.
- Define `Outcome` types that capture data callers need (paths created, VM handles, broker port, warnings). Include progress or warning events for streaming contexts (logs, managed image acquisition).
- Expose configuration discovery as a shared API, e.g. `ConfigSource::Discover | ::Explicit(PathBuf)`, returning detailed discovery reports.
- Clarify lifecycle expectations for long-lived processes: document how callers should shut down VMs/broker, and surface PID file locations in outcomes.
- Introduce an optional Cargo feature strategy (e.g., `default-features = ["cli"]`) so consumers can opt out of Clap/colour dependencies when using only the library surface.

## Stage 3 – CLI Adaptation

- Refactor `src/main.rs` to translate Clap input into the new option structs, call the library API, and render outcomes using existing formatting utilities (`src/app/display.rs`, `src/app/status.rs`).
- Reinstate all existing stdout/stderr content by mapping diagnostics and events back into the current textual form, maintaining ordering verified against Stage 0 transcripts.
- Wrap the hidden `broker` command around the new library function.
- Ensure exit-code behaviour matches today by converting library `Error` values into `ExitCode` via the CLI adapter.
- Update CLI tests to account for the new conversion layer, and add integration tests that assert stdout/exit codes for representative workflows.

## Stage 4 – Validation & Tooling

- Add unit tests targeting library APIs directly. Cover each option/outcome struct and ensure diagnostics are surfaced as expected.
- Include doctests or examples showing how to embed Castra programmatically (e.g., provisioning VMs from another application).
- Build a smoke-test harness that exercises CLI commands through the new library layer and diff outputs against Stage 0 transcripts.
- Configure CI to build and test both default (CLI + library) and library-only feature sets.

## Stage 5 – Documentation & Packaging

- Refresh `ARCHITECTURE.md` with the new layering diagram: `core` (library), `cli` (presentation), `managed` (data plane).
- Add library usage documentation (either a dedicated section in `README.md` or a `LIBRARY_USAGE.md`) covering configuration, running VMs, handling diagnostics, and cleanup.
- Provide contributor guidance describing where to add new diagnostics or events when extending functionality.
- Prepare release notes outlining renamed types (`CliError` → `Error`), new APIs, and feature flags; target at least a minor version bump for the public API shift.

## Risks & Open Questions

- Managed image acquisition currently streams events synchronously. Decide whether the library exposes them lazily (iterator/channel) or requires a reporter callback.
- Broker/VM lifecycle management must be explicit for library consumers. Explore returning handles or guard types that manage shutdown automatically.
- Downstream tools relying on exact CLI output should see no regression; any intentional changes must be documented.
- Optional dependency strategy: determine whether to gate `ureq`/network functionality behind features or keep it unconditional for now.

## Immediate Next Steps

1. Draft the architecture memo and collect CLI output baselines.
2. Sketch the public option/outcome structs and error enum for review before code changes.
3. Plan the extraction order (likely configuration/discovery first, then per-command flows) so each PR remains focused and reviewable.
