use std::path::PathBuf;
use std::time::Duration;

use crate::cli::DownArgs;
use crate::core::diagnostics::Severity;
use crate::core::events::{
    CooperativeMethod, CooperativeTimeoutReason, EphemeralCleanupReason, Event, ShutdownOutcome,
};
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
        workspace: args.workspace.clone(),
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
                CooperativeMethod::Acpi | CooperativeMethod::Agent => {
                    println!(
                        "→ {vm}: attempting cooperative shutdown via {} (wait up to {}).",
                        method.describe(),
                        format_duration_ms(*timeout_ms)
                    );
                }
                CooperativeMethod::Unavailable => {
                    println!(
                        "→ {vm}: cooperative shutdown unavailable ({}; wait {}). Escalating immediately.",
                        method.describe(),
                        format_duration_ms(*timeout_ms)
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
                    Some(detail) if !detail.is_empty() => println!(
                        "→ {vm}: cooperative shutdown {reason_text} after {} ({detail}).",
                        format_duration_ms(*waited_ms)
                    ),
                    _ => println!(
                        "→ {vm}: cooperative shutdown {reason_text} after {}.",
                        format_duration_ms(*waited_ms)
                    ),
                }

                if let Some(hint) = cooperative_hint(*reason) {
                    println!("   hint: {hint}");
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
            Event::EphemeralLayerDiscarded {
                vm,
                overlay_path,
                reclaimed_bytes,
                reason,
            } => match reason {
                EphemeralCleanupReason::Shutdown => println!(
                    "→ {vm}: ephemeral changes discarded (removed {} – {}). Export via SSH before `castra down` if you need to retain data.",
                    overlay_path.display(),
                    format_bytes(*reclaimed_bytes)
                ),
                EphemeralCleanupReason::Orphan => println!(
                    "→ {vm}: removed orphaned overlay {} (reclaimed {}).",
                    overlay_path.display(),
                    format_bytes(*reclaimed_bytes)
                ),
            },
            _ => {}
        }
    }
}

fn cooperative_hint(reason: CooperativeTimeoutReason) -> Option<&'static str> {
    match reason {
        CooperativeTimeoutReason::TimeoutExpired => None,
        CooperativeTimeoutReason::ChannelUnavailable => Some(
            "Enable the QMP powerdown channel (Castra-managed launches expose it automatically) or restart with `castra up` so the socket exists.",
        ),
        CooperativeTimeoutReason::ChannelError => Some(
            "Check the QMP socket path and permissions or restart the VM before retrying `castra down`.",
        ),
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

fn format_bytes(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = KIB * 1024.0;
    const GIB: f64 = MIB * 1024.0;

    if bytes < 1024 {
        return format!("{} B", bytes);
    }

    let value = bytes as f64;
    if value < MIB {
        return format!("{:.1} KiB", value / KIB);
    }
    if value < GIB {
        return format!("{:.1} MiB", value / MIB);
    }
    format!("{:.1} GiB", value / GIB)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cooperative_hint_provides_guidance_for_channel_unavailable() {
        let hint = cooperative_hint(CooperativeTimeoutReason::ChannelUnavailable)
            .expect("expected guidance");
        assert!(
            hint.contains("QMP"),
            "hint should mention QMP channel guidance: {hint}"
        );
    }

    #[test]
    fn cooperative_hint_is_none_for_timeout_expired() {
        assert!(cooperative_hint(CooperativeTimeoutReason::TimeoutExpired).is_none());
    }
}
