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

## Feature flags

- The crate enables the `cli` feature by default, which pulls in Clap and exposes the `castra::cli` and `castra::app` modules used by the bundled binary.
- Embedders that only need the library API should disable default features:

```toml
[dependencies]
castra = { version = "0.1.0", default-features = false }
```

- With `default-features = false`, only `castra::core` and the supporting config/error/managed types are compiled; CLI helpers stay out of the build.
