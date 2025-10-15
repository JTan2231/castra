use std::path::PathBuf;

use crate::Result;
use crate::cli::UpArgs;
use crate::core::diagnostics::Severity;
use crate::core::events::{
    BootstrapStatus, BootstrapStepKind, BootstrapStepStatus, BootstrapTrigger, Event,
    ManagedImageProfileComponents,
};
use crate::core::operations;
use crate::core::options::UpOptions;
use crate::core::outcome::{BootstrapRunStatus, UpOutcome};
use crate::core::project::format_config_warnings;
use castra::{ManagedImageProfileOutcome, ManagedImageVerificationOutcome};

use super::common::{config_load_options, emit_diagnostics, split_config_warnings};

pub fn handle_up(args: UpArgs, config_override: Option<&PathBuf>) -> Result<()> {
    let options = UpOptions {
        config: config_load_options(config_override, args.skip_discovery, "up")?,
        force: args.force,
    };

    let output = operations::up(options, None)?;

    let (config_warnings, other) = split_config_warnings(&output.diagnostics);
    if let Some(message) = format_config_warnings(&config_warnings) {
        eprint!("{message}");
    }
    emit_diagnostics(&other);

    render_up(&output.value, &output.events);

    Ok(())
}

fn render_up(outcome: &UpOutcome, events: &[Event]) {
    for event in events {
        match event {
            Event::ManagedArtifact { spec, text, .. } => {
                println!("→ {} {}: {}", spec.id, spec.version, text);
            }
            Event::ManagedImageVerificationStarted { spec, plan, .. } => {
                let kinds: Vec<&str> = plan
                    .iter()
                    .map(|artifact| artifact.kind.describe())
                    .collect();
                if kinds.is_empty() {
                    println!("→ {} {}: verification started.", spec.id, spec.version);
                } else {
                    println!(
                        "→ {} {}: verification started for {}.",
                        spec.id,
                        spec.version,
                        kinds.join(", ")
                    );
                }
            }
            Event::ManagedImageVerificationResult {
                spec,
                duration_ms,
                outcome,
                artifacts,
                ..
            } => {
                let kinds: Vec<&str> = artifacts
                    .iter()
                    .map(|artifact| artifact.kind.describe())
                    .collect();
                let duration = format_duration_ms(*duration_ms);
                match outcome {
                    ManagedImageVerificationOutcome::Success => {
                        if kinds.is_empty() {
                            println!(
                                "→ {} {}: verification completed in {}.",
                                spec.id, spec.version, duration
                            );
                        } else {
                            println!(
                                "→ {} {}: verification completed in {} ({}).",
                                spec.id,
                                spec.version,
                                duration,
                                kinds.join(", ")
                            );
                        }
                    }
                    ManagedImageVerificationOutcome::Failure { reason } => {
                        println!(
                            "→ {} {}: verification failed after {} ({}).",
                            spec.id, spec.version, duration, reason
                        );
                    }
                }
            }
            Event::ManagedImageProfileApplied {
                spec,
                vm,
                components,
                ..
            } => {
                let labels = describe_profile_components(components);
                println!(
                    "→ {} {}: applying boot profile for VM `{}` ({}).",
                    spec.id,
                    spec.version,
                    vm,
                    labels.join(", ")
                );
            }
            Event::ManagedImageProfileResult {
                spec,
                vm,
                duration_ms,
                outcome,
                components,
                ..
            } => {
                let labels = describe_profile_components(components);
                let duration = format_duration_ms(*duration_ms);
                match outcome {
                    ManagedImageProfileOutcome::Applied => {
                        println!(
                            "→ {} {}: boot profile applied to `{}` in {} ({}).",
                            spec.id,
                            spec.version,
                            vm,
                            duration,
                            labels.join(", ")
                        );
                    }
                    ManagedImageProfileOutcome::NoOp => {
                        println!(
                            "→ {} {}: boot profile skipped (no changes needed).",
                            spec.id, spec.version
                        );
                    }
                    ManagedImageProfileOutcome::Failed { reason } => {
                        println!(
                            "→ {} {}: boot profile failed for `{}` ({reason}).",
                            spec.id, spec.version, vm
                        );
                    }
                }
            }
            Event::OverlayPrepared { vm, overlay_path } => {
                println!(
                    "Prepared overlay for VM `{vm}` at {}.",
                    overlay_path.display()
                );
            }
            Event::VmLaunched { vm, .. } => {
                let pidfile = outcome.state_root.join(format!("{vm}.pid"));
                println!("→ {vm}: launched (pidfile {}).", pidfile.display());
            }
            Event::BootstrapStarted {
                vm,
                base_hash,
                artifact_hash,
                trigger,
            } => {
                println!(
                    "→ {}: bootstrap started (artifact {}, base {}) [{}].",
                    vm,
                    hash_snippet(artifact_hash),
                    hash_snippet(base_hash),
                    format_bootstrap_trigger(trigger)
                );
            }
            Event::BootstrapStep {
                vm,
                step,
                status,
                duration_ms,
                detail,
            } => {
                let duration = format_duration_ms(*duration_ms);
                match detail {
                    Some(text) if !text.is_empty() => println!(
                        "   - {} {}: {} in {} ({}).",
                        vm,
                        format_step_kind(step),
                        format_step_status(status),
                        duration,
                        text
                    ),
                    _ => println!(
                        "   - {} {}: {} in {}.",
                        vm,
                        format_step_kind(step),
                        format_step_status(status),
                        duration
                    ),
                }
            }
            Event::BootstrapCompleted {
                vm,
                status,
                duration_ms,
                stamp,
            } => {
                let duration = format_duration_ms(*duration_ms);
                let stamp_label = stamp.as_deref().unwrap_or("n/a");
                match status {
                    BootstrapStatus::Success => {
                        println!(
                            "→ {}: bootstrap completed in {} (stamp {}).",
                            vm, duration, stamp_label
                        );
                    }
                    BootstrapStatus::NoOp => {
                        println!("→ {}: bootstrap up-to-date (stamp {}).", vm, stamp_label);
                    }
                }
            }
            Event::BootstrapFailed {
                vm,
                duration_ms,
                error,
            } => {
                let duration = format_duration_ms(*duration_ms);
                eprintln!(
                    "Bootstrap failed for `{}` after {}: {}",
                    vm, duration, error
                );
            }
            Event::BrokerStarted { pid, port } => {
                println!("→ broker: launched on 127.0.0.1:{port} (pid {pid}).");
            }
            Event::Message { severity, text } => match severity {
                Severity::Info => println!("{}", text),
                Severity::Warning => eprintln!("Warning: {}", text),
                Severity::Error => eprintln!("Error: {}", text),
            },
            _ => {}
        }
    }

    if !outcome.bootstraps.is_empty() {
        for run in &outcome.bootstraps {
            match run.status {
                BootstrapRunStatus::Success => {
                    let stamp = run.stamp.as_deref().unwrap_or("n/a");
                    match &run.log_path {
                        Some(path) => println!(
                            "→ {}: bootstrap log at {} (stamp {}).",
                            run.vm,
                            path.display(),
                            stamp
                        ),
                        None => println!("→ {}: bootstrap completed (stamp {}).", run.vm, stamp),
                    }
                }
                BootstrapRunStatus::NoOp => {
                    let stamp = run.stamp.as_deref().unwrap_or("n/a");
                    println!("→ {}: bootstrap no-op (stamp {}).", run.vm, stamp);
                }
                BootstrapRunStatus::Skipped => {
                    println!("→ {}: bootstrap skipped.", run.vm);
                }
            }
        }
    }
}

fn describe_profile_components(components: &ManagedImageProfileComponents) -> Vec<String> {
    let mut labels = vec!["kernel".to_string()];
    if components.initrd.is_some() {
        labels.push("initrd".to_string());
    }
    if !components.append.is_empty() {
        labels.push("append".to_string());
    }
    if !components.extra_args.is_empty() {
        labels.push(format!("extra_args={}", components.extra_args.len()));
    }
    if let Some(machine) = &components.machine {
        labels.push(format!("machine={machine}"));
    }
    labels
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

fn hash_snippet(value: &str) -> String {
    if value.len() <= 12 {
        value.to_string()
    } else {
        format!("{}…", &value[..12])
    }
}

fn format_bootstrap_trigger(trigger: &BootstrapTrigger) -> &'static str {
    match trigger {
        BootstrapTrigger::Auto => "auto",
        BootstrapTrigger::Always => "always",
    }
}

fn format_step_kind(kind: &BootstrapStepKind) -> &'static str {
    match kind {
        BootstrapStepKind::WaitHandshake => "wait-handshake",
        BootstrapStepKind::Connect => "connect",
        BootstrapStepKind::Transfer => "transfer",
        BootstrapStepKind::Apply => "apply",
        BootstrapStepKind::Verify => "verify",
    }
}

fn format_step_status(status: &BootstrapStepStatus) -> &'static str {
    match status {
        BootstrapStepStatus::Success => "success",
        BootstrapStepStatus::Skipped => "skipped",
        BootstrapStepStatus::Failed => "failed",
    }
}
