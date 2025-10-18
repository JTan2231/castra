use std::path::PathBuf;

use crate::Error;
use crate::Result;
use crate::cli::{BootstrapOverrideArg, UpArgs};
use crate::core::diagnostics::Severity;
use crate::core::events::{
    BootstrapPlanAction, BootstrapStatus, BootstrapStepKind, BootstrapStepStatus, BootstrapTrigger,
    Event,
};
use crate::core::operations;
use crate::core::options::{BootstrapOverrides, UpOptions};
use crate::core::outcome::{BootstrapRunStatus, UpOutcome};
use crate::core::project::format_config_warnings;

use super::common::{config_load_options, emit_diagnostics, split_config_warnings};

pub fn handle_up(args: UpArgs, config_override: Option<&PathBuf>) -> Result<()> {
    let UpArgs {
        skip_discovery,
        force,
        plan,
        qcow,
        bootstrap,
    } = args;

    let bootstrap_overrides = build_bootstrap_overrides(&bootstrap)?;
    let options = UpOptions {
        config: config_load_options(config_override, skip_discovery, "up")?,
        force,
        bootstrap: bootstrap_overrides,
        plan,
        alpine_qcow_override: qcow,
    };

    let output = operations::up(options, None)?;

    let (config_warnings, other) = split_config_warnings(&output.diagnostics);
    if let Some(message) = format_config_warnings(&config_warnings) {
        eprint!("{message}");
    }
    emit_diagnostics(&other);

    render_up(&output.value, &output.events);

    if plan {
        let failed: Vec<&str> = output
            .value
            .plans
            .iter()
            .filter(|plan| plan.action == BootstrapPlanAction::Error)
            .map(|plan| plan.vm.as_str())
            .collect();

        if !failed.is_empty() {
            let joined = failed.join(", ");
            return Err(Error::PreflightFailed {
                message: format!(
                    "Bootstrap plan detected configuration errors for {joined}. Resolve them before running without --plan."
                ),
            });
        }
    }

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
            Event::BootstrapPlanned {
                vm,
                mode,
                action,
                reason,
                trigger,
                script_path,
                payload_path,
                payload_bytes,
                handshake_timeout_secs,
                remote_dir,
                ssh,
                env_keys,
                verify,
                artifact_hash,
                metadata_path,
                warnings,
            } => {
                let mode_text = mode.as_str();
                match action {
                    BootstrapPlanAction::WouldRun => {
                        println!("→ {}: plan would run ({}; {}).", vm, mode_text, reason);
                    }
                    BootstrapPlanAction::WouldSkip => {
                        println!("→ {}: plan would skip ({}; {}).", vm, mode_text, reason);
                    }
                    BootstrapPlanAction::Error => {
                        eprintln!("→ {}: plan would error ({}; {}).", vm, mode_text, reason);
                    }
                }

                if let Some(path) = script_path {
                    println!("   script: {}", path.display());
                }

                if let Some(seconds) = handshake_timeout_secs {
                    println!("   handshake wait: {}s", seconds);
                }

                if let Some(dir) = remote_dir {
                    println!("   remote dir: {}", dir);
                }

                if let Some(ssh) = ssh {
                    let mut summary = ssh.summary();
                    if let Some(identity) = &ssh.identity {
                        summary = format!("{} (identity: {})", summary, identity.display());
                    }
                    println!("   ssh: {}", summary);
                    if !ssh.options.is_empty() {
                        println!("   ssh options: {}", ssh.options.join(", "));
                    }
                }

                let payload_path_ref = payload_path.as_ref();
                let payload_bytes_value = payload_bytes.as_ref().copied();

                match (payload_path_ref, payload_bytes_value) {
                    (Some(path), Some(bytes)) => {
                        println!("   payload: {} ({}).", path.display(), format_bytes(bytes));
                    }
                    (Some(path), None) => {
                        println!("   payload: {}.", path.display());
                    }
                    (None, Some(bytes)) if bytes > 0 => {
                        println!("   payload size: {} (path missing).", format_bytes(bytes));
                    }
                    _ => {}
                }

                if !env_keys.is_empty() {
                    println!("   env keys: {}", env_keys.join(", "));
                }

                if let Some(verify) = verify {
                    let mut parts = Vec::new();
                    if let Some(cmd) = &verify.command {
                        parts.push(format!("command={cmd}"));
                    }
                    if let Some(path) = &verify.path {
                        let scope = if verify.path_is_relative {
                            "relative"
                        } else {
                            "absolute"
                        };
                        parts.push(format!("path={} ({scope})", path));
                    }
                    if !parts.is_empty() {
                        println!("   verify: {}", parts.join(", "));
                    }
                }

                if let Some(hash) = artifact_hash {
                    println!("   artifact: {}", hash_snippet(hash.as_str()));
                }

                if let Some(path) = metadata_path {
                    println!("   metadata: {}", path.display());
                }

                for warning in warnings {
                    println!("   ! {warning}");
                }

                if let Some(trigger) = trigger {
                    println!("   trigger: {}", format_bootstrap_trigger(trigger));
                }
            }
            Event::EphemeralLayerDiscarded {
                vm,
                overlay_path,
                reclaimed_bytes,
                reason,
            } => match reason {
                crate::core::events::EphemeralCleanupReason::Orphan => println!(
                    "→ {vm}: removed stale overlay {} (reclaimed {}).",
                    overlay_path.display(),
                    format_bytes(*reclaimed_bytes)
                ),
                crate::core::events::EphemeralCleanupReason::Shutdown => println!(
                    "→ {vm}: discarded ephemeral overlay {} (reclaimed {}).",
                    overlay_path.display(),
                    format_bytes(*reclaimed_bytes)
                ),
            },
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
                ..
            } => {
                let duration = format_duration_ms(*duration_ms);
                match status {
                    BootstrapStatus::Success => {
                        println!("→ {}: bootstrap completed in {}.", vm, duration);
                    }
                    BootstrapStatus::NoOp => {
                        println!("→ {}: bootstrap runner reported no changes.", vm);
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

    if !outcome.plans.is_empty() {
        let run = outcome
            .plans
            .iter()
            .filter(|plan| plan.action == BootstrapPlanAction::WouldRun)
            .count();
        let skip = outcome
            .plans
            .iter()
            .filter(|plan| plan.action == BootstrapPlanAction::WouldSkip)
            .count();
        let errors = outcome
            .plans
            .iter()
            .filter(|plan| plan.action == BootstrapPlanAction::Error)
            .count();
        println!(
            "Bootstrap plan summary: {run} would run, {skip} would skip, {errors} would error."
        );
    }

    if !outcome.bootstraps.is_empty() {
        for run in &outcome.bootstraps {
            match run.status {
                BootstrapRunStatus::Success => match &run.log_path {
                    Some(path) => println!("→ {}: bootstrap log at {}.", run.vm, path.display()),
                    None => println!("→ {}: bootstrap completed.", run.vm),
                },
                BootstrapRunStatus::NoOp => {
                    println!("→ {}: bootstrap runner reported no changes.", run.vm);
                }
                BootstrapRunStatus::Skipped => {
                    println!("→ {}: bootstrap skipped.", run.vm);
                }
            }
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
            BootstrapOverrideArg::Global(BootstrapMode::Skip),
            BootstrapOverrideArg::Vm {
                vm: "api-0".to_string(),
                mode: BootstrapMode::Always,
            },
        ])
        .expect("build overrides");

        assert_eq!(overrides.global, Some(BootstrapMode::Skip));
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
                mode: BootstrapMode::Skip,
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
