//! Minimal embedding example demonstrating castra-core operations.

use std::env;
use std::path::PathBuf;

use castra::{
    Error,
    core::{
        diagnostics::{Diagnostic, Severity},
        events::Event,
        operations,
        options::{ConfigLoadOptions, UpOptions},
        outcome::UpOutcome,
        reporter::Reporter,
    },
};

fn main() -> Result<(), Error> {
    let ExampleConfig { config_path, plan } = parse_args()?;

    let mut options = UpOptions::default();
    options.config = ConfigLoadOptions::explicit(config_path);
    options.plan = plan;

    let mut reporter = StdoutReporter;
    let output = operations::up(options, Some(&mut reporter))?;

    emit_diagnostics(&output.diagnostics);
    summarize_outcome(&output.value);

    Ok(())
}

struct ExampleConfig {
    config_path: PathBuf,
    plan: bool,
}

fn parse_args() -> Result<ExampleConfig, Error> {
    let mut args = env::args().skip(1);
    let mut config_override: Option<PathBuf> = None;
    let mut plan = true;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--config" => {
                let value = args.next().ok_or_else(|| Error::PreflightFailed {
                    message: "--config requires a path".to_string(),
                })?;
                config_override = Some(PathBuf::from(value));
            }
            "--execute" => plan = false,
            "--plan" => plan = true,
            other => {
                return Err(Error::PreflightFailed {
                    message: format!(
                        "Unknown argument `{other}`. Use --config <path>, --cli <path>, --plan, or --execute."
                    ),
                });
            }
        }
    }

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let default_config = manifest_dir.join("examples/bootstrap-quickstart/castra.toml");
    let config_path = config_override.unwrap_or(default_config);
    let config_path = config_path
        .canonicalize()
        .map_err(|err| Error::PreflightFailed {
            message: format!(
                "Unable to resolve config at {}: {err}",
                config_path.display()
            ),
        })?;

    Ok(ExampleConfig { config_path, plan })
}

struct StdoutReporter;

impl Reporter for StdoutReporter {
    fn report(&mut self, event: Event) {
        println!("event: {event:?}");
    }
}

fn emit_diagnostics(diagnostics: &[Diagnostic]) {
    if diagnostics.is_empty() {
        return;
    }
    println!("diagnostics:");
    for diagnostic in diagnostics {
        let prefix = match diagnostic.severity {
            Severity::Info => "INFO",
            Severity::Warning => "WARN",
            Severity::Error => "ERROR",
        };
        println!("  [{prefix}] {}", diagnostic.message);
    }
}

fn summarize_outcome(outcome: &UpOutcome) {
    println!(
        "outcome: {} VM(s) launched; bootstrap runs: {}",
        outcome.launched_vms.len(),
        outcome.bootstraps.len()
    );
}
