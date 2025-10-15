use std::path::PathBuf;

use crate::Error;
use crate::Result;
use crate::cli::{BootstrapOverrideArg, UpArgs};
use crate::core::diagnostics::Severity;
use crate::core::events::{
    BootstrapStatus, BootstrapStepKind, BootstrapStepStatus, BootstrapTrigger, Event,
};
use crate::core::operations;
use crate::core::options::{BootstrapOverrides, UpOptions};
use crate::core::outcome::{BootstrapRunStatus, UpOutcome};
use crate::core::project::format_config_warnings;
use castra::{ManagedImageProfileOutcome, ManagedImageVerificationOutcome};

use super::common::{config_load_options, emit_diagnostics, split_config_warnings};

pub fn handle_up(args: UpArgs, config_override: Option<&PathBuf>) -> Result<()> {
    let bootstrap_overrides = build_bootstrap_overrides(&args.bootstrap)?;
    let options = UpOptions {
        config: config_load_options(config_override, args.skip_discovery, "up")?,
        force: args.force,
        bootstrap: bootstrap_overrides,
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

fn build_bootstrap_overrides(inputs: &[BootstrapOverrideArg]) -> Result<BootstrapOverrides> {
    let mut overrides = BootstrapOverrides::default();

    for entry in inputs {
        match entry {
            BootstrapOverrideArg::Global(mode) => {
                if let Some(existing) = overrides.global {
                    if existing != *mode {
                        return Err(Error::PreflightFailed {
                            message: format!(
                                "Conflicting global bootstrap overrides `{}` and `{}`.",
                                existing.as_str(),
                                mode.as_str()
                            ),
                        });
                    }
                } else {
                    overrides.global = Some(*mode);
                }
            }
            BootstrapOverrideArg::Vm { vm, mode } => {
                if let Some(existing) = overrides.per_vm.get(vm) {
                    if existing != mode {
                        return Err(Error::PreflightFailed {
                            message: format!(
                                "Conflicting bootstrap overrides for `{vm}`: `{}` vs `{}`.",
                                existing.as_str(),
                                mode.as_str()
                            ),
                        });
                    }
                } else {
                    overrides.per_vm.insert(vm.clone(), *mode);
                }
            }
        }
    }

    Ok(overrides)
}

fn render_up(outcome: &UpOutcome, events: &[Event]) {
    for event in events {
        match event {
            Event::ManagedArtifact { spec, text, .. } => {
                println!("→ {} {}: {}", spec.id, spec.version, text);
            }
            Event::ManagedImageVerificationStarted {
                image_id,
                image_version,
                plan,
                ..
            } => {
                let kinds: Vec<&str> = plan
                    .iter()
                    .map(|artifact| artifact.kind.describe())
                    .collect();
                if kinds.is_empty() {
                    println!("→ {} {}: verification started.", image_id, image_version);
                } else {
                    println!(
                        "→ {} {}: verification started for {}.",
                        image_id,
                        image_version,
                        kinds.join(", ")
                    );
                }
            }
            Event::ManagedImageVerificationResult {
                image_id,
                image_version,
                duration_ms,
                outcome,
                error,
                size_bytes,
                artifacts,
                ..
            } => {
                let kinds: Vec<&str> = artifacts
                    .iter()
                    .map(|artifact| artifact.kind.describe())
                    .collect();
                let duration = format_duration_ms(*duration_ms);
                let size_text = format_bytes(*size_bytes);
                match outcome {
                    ManagedImageVerificationOutcome::Success => {
                        if kinds.is_empty() {
                            println!(
                                "→ {} {}: verification completed in {} ({}).",
                                image_id, image_version, duration, size_text
                            );
                        } else {
                            println!(
                                "→ {} {}: verification completed in {} ({}; {}).",
                                image_id,
                                image_version,
                                duration,
                                size_text,
                                kinds.join(", ")
                            );
                        }
                    }
                    ManagedImageVerificationOutcome::Failure { reason } => {
                        let detail = error.as_deref().unwrap_or(reason);
                        println!(
                            "→ {} {}: verification failed after {} ({}).",
                            image_id, image_version, duration, detail
                        );
                    }
                }
            }
            Event::ManagedImageProfileApplied {
                image_id,
                image_version,
                vm,
                profile_id,
                steps,
                ..
            } => {
                println!(
                    "→ {} {}: applying profile `{}` to VM `{}` ({}).",
                    image_id,
                    image_version,
                    profile_id,
                    vm,
                    format_steps(steps)
                );
            }
            Event::ManagedImageProfileResult {
                image_id,
                image_version,
                vm,
                profile_id,
                duration_ms,
                outcome,
                error,
                steps,
                ..
            } => {
                let duration = format_duration_ms(*duration_ms);
                match outcome {
                    ManagedImageProfileOutcome::Applied => {
                        println!(
                            "→ {} {}: profile `{}` applied to `{}` in {} ({}).",
                            image_id,
                            image_version,
                            profile_id,
                            vm,
                            duration,
                            format_steps(steps)
                        );
                    }
                    ManagedImageProfileOutcome::NoOp => {
                        println!(
                            "→ {} {}: profile `{}` skipped (no changes needed).",
                            image_id, image_version, profile_id
                        );
                    }
                    ManagedImageProfileOutcome::Failed { reason } => {
                        let detail = error.as_deref().unwrap_or(reason);
                        println!(
                            "→ {} {}: profile `{}` failed for `{}` ({detail}).",
                            image_id, image_version, profile_id, vm
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

fn format_steps(steps: &[String]) -> String {
    if steps.is_empty() {
        "no steps".to_string()
    } else {
        steps.join(", ")
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
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut index = 0usize;
    while value >= 1024.0 && index < UNITS.len() - 1 {
        value /= 1024.0;
        index += 1;
    }
    if index == 0 {
        format!("{bytes} {}", UNITS[index])
    } else {
        format!("{value:.1} {}", UNITS[index])
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

#[cfg(test)]
mod tests {
    use super::*;
    use castra::BootstrapMode;

    #[test]
    fn build_bootstrap_overrides_supports_global_and_vm_specific_modes() {
        let overrides = build_bootstrap_overrides(&[
            BootstrapOverrideArg::Global(BootstrapMode::Disabled),
            BootstrapOverrideArg::Vm {
                vm: "api-0".to_string(),
                mode: BootstrapMode::Always,
            },
        ])
        .expect("build overrides");

        assert_eq!(overrides.global, Some(BootstrapMode::Disabled));
        assert_eq!(overrides.per_vm.get("api-0"), Some(&BootstrapMode::Always));
    }

    #[test]
    fn build_bootstrap_overrides_detects_conflicting_global_modes() {
        let err = build_bootstrap_overrides(&[
            BootstrapOverrideArg::Global(BootstrapMode::Auto),
            BootstrapOverrideArg::Global(BootstrapMode::Always),
        ])
        .unwrap_err();

        match err {
            Error::PreflightFailed { message } => {
                assert!(
                    message.contains("Conflicting global bootstrap overrides"),
                    "unexpected message: {message}"
                );
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn build_bootstrap_overrides_detects_conflicting_vm_modes() {
        let err = build_bootstrap_overrides(&[
            BootstrapOverrideArg::Vm {
                vm: "api-0".to_string(),
                mode: BootstrapMode::Auto,
            },
            BootstrapOverrideArg::Vm {
                vm: "api-0".to_string(),
                mode: BootstrapMode::Disabled,
            },
        ])
        .unwrap_err();

        match err {
            Error::PreflightFailed { message } => {
                assert!(
                    message.contains("Conflicting bootstrap overrides for `api-0`"),
                    "unexpected message: {message}"
                );
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }
}
