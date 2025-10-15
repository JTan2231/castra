use std::path::PathBuf;
use std::time::Duration;

use crate::cli::DownArgs;
use crate::core::diagnostics::Severity;
use crate::core::events::{CooperativeMethod, Event, ShutdownOutcome};
use crate::core::operations;
use crate::core::options::DownOptions;
use crate::core::project::format_config_warnings;
use crate::{Error, Result};

use super::common::{config_load_options, emit_diagnostics, split_config_warnings};

pub fn handle_down(args: DownArgs, config_override: Option<&PathBuf>) -> Result<()> {
    if let Some(value) = args.sigkill_wait_secs {
        if value == 0 {
            return Err(Error::PreflightFailed {
                message: "Override --sigkill-wait-secs must be at least 1 to confirm guest exit."
                    .into(),
            });
        }
    }

    let options = DownOptions {
        config: config_load_options(config_override, args.skip_discovery, "down")?,
        graceful_wait: args.graceful_wait_secs.map(Duration::from_secs),
        sigterm_wait: args.sigterm_wait_secs.map(Duration::from_secs),
        sigkill_wait: args.sigkill_wait_secs.map(Duration::from_secs),
    };

    let output = operations::down(options, None)?;

    let (config_warnings, other) = split_config_warnings(&output.diagnostics);
    if let Some(message) = format_config_warnings(&config_warnings) {
        eprint!("{message}");
    }
    emit_diagnostics(&other);

    render_down(&output.events);

    let forced: Vec<String> = output
        .value
        .vm_results
        .iter()
        .filter(|vm| vm.changed && vm.outcome == ShutdownOutcome::Forced)
        .map(|vm| vm.name.clone())
        .collect();

    if !forced.is_empty() {
        eprintln!(
            "Warning: forced shutdown required for {}.",
            forced.join(", ")
        );
    }

    Ok(())
}

fn render_down(events: &[Event]) {
    for event in events {
        match event {
            Event::Message { severity, text } => match severity {
                Severity::Info => println!("{text}"),
                Severity::Warning => eprintln!("Warning: {text}"),
                Severity::Error => eprintln!("Error: {text}"),
            },
            Event::ShutdownRequested { vm } => {
                println!("→ {vm}: shutdown requested.");
            }
            Event::CooperativeAttempted {
                vm,
                method,
                timeout_ms,
            } => match method {
                CooperativeMethod::Acpi => {
                    println!(
                        "→ {vm}: attempting cooperative shutdown via {} (wait up to {}).",
                        method.describe(),
                        format_duration_ms(*timeout_ms)
                    );
                }
                CooperativeMethod::Agent => {
                    println!(
                        "→ {vm}: attempting cooperative shutdown via {} (wait up to {}).",
                        method.describe(),
                        format_duration_ms(*timeout_ms)
                    );
                }
                CooperativeMethod::Unavailable => {
                    println!(
                        "→ {vm}: no cooperative shutdown channel available; proceeding to host termination."
                    );
                }
            },
            Event::CooperativeSucceeded { vm, elapsed_ms } => {
                println!(
                    "→ {vm}: guest confirmed shutdown in {}.",
                    format_duration_ms(*elapsed_ms)
                );
            }
            Event::CooperativeTimedOut {
                vm,
                waited_ms,
                reason,
                detail,
            } => {
                let reason_text = reason.describe();
                match detail {
                    Some(detail) if !detail.is_empty() => {
                        println!(
                            "→ {vm}: cooperative shutdown {reason_text} after {} ({detail}).",
                            format_duration_ms(*waited_ms)
                        );
                    }
                    _ => {
                        println!(
                            "→ {vm}: cooperative shutdown {reason_text} after {}.",
                            format_duration_ms(*waited_ms)
                        );
                    }
                }
            }
            Event::ShutdownEscalated {
                vm,
                signal,
                timeout_ms,
            } => {
                if let Some(ms) = timeout_ms {
                    println!(
                        "→ {vm}: escalating to {}; waiting up to {}.",
                        signal.describe(),
                        format_duration_ms(*ms)
                    );
                } else {
                    println!("→ {vm}: escalating to {}.", signal.describe());
                }
            }
            Event::ShutdownComplete {
                vm,
                outcome,
                changed,
                total_ms,
            } => {
                if !changed {
                    println!(
                        "→ {vm}: already stopped (checked in {}).",
                        format_duration_ms(*total_ms)
                    );
                } else {
                    match outcome {
                        ShutdownOutcome::Graceful => {
                            println!(
                                "→ {vm}: stopped (graceful) in {}.",
                                format_duration_ms(*total_ms)
                            );
                        }
                        ShutdownOutcome::Forced => {
                            println!(
                                "→ {vm}: stopped (forced) in {}.",
                                format_duration_ms(*total_ms)
                            );
                        }
                    }
                }
            }
            Event::BrokerStopped { changed } => {
                if !changed {
                    println!("→ broker: already stopped.");
                } else {
                    println!("→ broker: stopped.");
                }
            }
            _ => {}
        }
    }
}

fn format_duration_ms(ms: u64) -> String {
    if ms == 0 {
        return "0s".to_string();
    }

    if ms % 1000 == 0 {
        return format!("{}s", ms / 1000);
    }

    let seconds = ms as f64 / 1000.0;
    if seconds >= 1.0 {
        format!("{seconds:.1}s")
    } else {
        format!("{ms}ms")
    }
}
