# Castra Library Usage

The Castra crate now exposes first-class library APIs under the `castra::core` namespace. Each CLI workflow maps to a function in `castra::core::operations` that accepts an options struct and returns an `OperationOutput<T>` with a typed outcome, structured diagnostics, and a list of progress events.

```rust
use castra::core::{operations, options::StatusOptions};

fn main() -> castra::Result<()> {
    let output = operations::status(StatusOptions::default(), None)?;

    for diagnostic in &output.diagnostics {
        eprintln!("{:?}: {}", diagnostic.severity, diagnostic.message);
    }

    println!("project: {}", output.value.project_name);
    Ok(())
}
```

- Options live under `castra::core::options` (`InitOptions`, `UpOptions`, etc.).
- Outcomes live under `castra::core::outcome` (`InitOutcome`, `UpOutcome`, `StatusOutcome`, etc.).
- Progress events are emitted as `castra::core::events::Event` values so callers can stream user-facing progress or implement custom logging.

The CLI now acts as a thin adapter that translates Clap arguments into these option structures, delegates work to `castra::core::operations`, and renders diagnostics/events using the existing formatting helpers in `src/app`.
