use std::collections::{BTreeMap, HashMap};
use std::fs::{self, File};
use std::io::{self, Read};
use std::panic;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::config::PortProtocol;
use crate::config::{BootstrapMode, ProjectConfig, VmDefinition};
use crate::core::diagnostics::{Diagnostic, Severity};
use crate::core::events::{
    BootstrapPlanAction, BootstrapPlanSsh, BootstrapPlanVerify, BootstrapStatus, BootstrapStepKind,
    BootstrapStepStatus, BootstrapTrigger, Event,
};
use crate::core::outcome::{BootstrapPlanOutcome, BootstrapRunOutcome, BootstrapRunStatus};
use crate::core::reporter::Reporter;
use crate::core::runtime::{AssetPreparation, RuntimeContext};
use crate::core::status::HANDSHAKE_FRESHNESS;
use crate::error::{Error, Result};
use sha2::{Digest, Sha256};

const LOG_SUBDIR: &str = "bootstrap";
const STAGING_SUBDIR: &str = "bootstrap";
const STAGED_SCRIPT_NAME: &str = "run.sh";
const STAGED_PAYLOAD_DIR: &str = "payload";
const DEFAULT_REMOTE_BASE: &str = "/tmp/castra-bootstrap";
const DEFAULT_SSH_USER: &str = "root";
const DEFAULT_SSH_HOST: &str = "127.0.0.1";
const DEFAULT_SSH_PORT: u16 = 22;
const DEFAULT_SSH_OPTIONS: [&str; 2] = ["StrictHostKeyChecking=no", "UserKnownHostsFile=/dev/null"];
const SENTINEL_NOOP: &str = "Castra:noop";
const SENTINEL_ERROR_PREFIX: &str = "Castra:error:";

/// Execute bootstrap pipelines for all VMs in the project, returning per-VM summaries.
pub fn run_all(
    project: &ProjectConfig,
    context: &RuntimeContext,
    preparations: &[AssetPreparation],
    reporter: &mut dyn Reporter,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<BootstrapRunOutcome>> {
    if project.vms.len() != preparations.len() {
        return Err(Error::PreflightFailed {
            message: format!(
                "Bootstrap preparation mismatch: expected {} VMs but received {} asset sets.",
                project.vms.len(),
                preparations.len()
            ),
        });
    }

    let active_vm_names: Vec<String> = project
        .vms
        .iter()
        .filter(|vm| !matches!(vm.bootstrap.mode, BootstrapMode::Skip))
        .map(|vm| vm.name.clone())
        .collect();
    if !active_vm_names.is_empty() {
        let list = active_vm_names.join(", ");
        reporter.report(Event::Message {
            severity: Severity::Info,
            text: format!(
                "Bootstrap starting for {} VM(s): {}.",
                active_vm_names.len(),
                list
            ),
        });
    }

    let (event_tx, event_rx) = mpsc::channel::<Event>();
    let mut first_error: Option<Error> = None;
    let mut vm_slots: Vec<Option<BootstrapRunOutcome>> =
        (0..project.vms.len()).map(|_| None).collect();

    let state_root = context.state_root.clone();
    let log_root = context.log_root.clone();

    std::thread::scope(|scope| {
        let mut handles = Vec::new();

        for (index, (vm, _prep)) in project.vms.iter().zip(preparations.iter()).enumerate() {
            let tx_clone = event_tx.clone();
            let state_root = state_root.clone();
            let log_root = log_root.clone();
            handles.push(scope.spawn(move || {
                let mut local_diagnostics = Vec::new();
                let mut emit_event = |event: Event| {
                    let _ = tx_clone.send(event);
                };
                let outcome = run_for_vm(
                    &state_root,
                    &log_root,
                    vm,
                    &mut emit_event,
                    &mut local_diagnostics,
                );
                (index, outcome, local_diagnostics)
            }));
        }

        drop(event_tx);

        while let Ok(event) = event_rx.recv() {
            reporter.report(event);
        }

        for handle in handles {
            match handle.join() {
                Ok((index, outcome_result, local_diagnostics)) => {
                    diagnostics.extend(local_diagnostics);
                    match outcome_result {
                        Ok(outcome) => {
                            vm_slots[index] = Some(outcome);
                        }
                        Err(err) => {
                            if first_error.is_none() {
                                first_error = Some(err);
                            }
                        }
                    }
                }
                Err(payload) => panic::resume_unwind(payload),
            }
        }
    });

    if let Some(err) = first_error {
        return Err(err);
    }

    Ok(vm_slots
        .into_iter()
        .map(|slot| {
            slot.unwrap_or_else(|| {
                panic!("bootstrap worker did not produce a result for configured VM")
            })
        })
        .collect())
}

/// Produce dry-run summaries for bootstrap pipelines without side effects.
pub fn plan_all(
    project: &ProjectConfig,
    reporter: &mut dyn Reporter,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<BootstrapPlanOutcome>> {
    let mut plans = Vec::new();

    for vm in &project.vms {
        let plan = plan_for_vm(vm);
        if plan.action.is_error() {
            diagnostics.push(
                Diagnostic::new(
                    Severity::Error,
                    format!(
                        "Bootstrap plan for `{}` would error: {}",
                        plan.vm, plan.reason
                    ),
                )
                .with_help(
                    "Update bootstrap configuration before running `castra up` without --plan.",
                ),
            );
        }

        reporter.report(Event::BootstrapPlanned {
            vm: plan.vm.clone(),
            mode: plan.mode,
            action: plan.action,
            reason: plan.reason.clone(),
            trigger: plan.trigger,
            script_path: plan.script_path.clone(),
            payload_path: plan.payload_path.clone(),
            payload_bytes: plan.payload_bytes,
            handshake_timeout_secs: plan.handshake_timeout_secs,
            remote_dir: plan.remote_dir.clone(),
            ssh: plan.ssh.clone(),
            env_keys: plan.env_keys.clone(),
            verify: plan.verify.clone(),
            artifact_hash: plan.artifact_hash.clone(),
            metadata_path: plan.metadata_path.clone(),
            warnings: plan.warnings.clone(),
        });

        plans.push(plan);
    }

    Ok(plans)
}

fn plan_for_vm(vm: &VmDefinition) -> BootstrapPlanOutcome {
    let mode = vm.bootstrap.mode;
    match mode {
        BootstrapMode::Skip => {
            return BootstrapPlanOutcome {
                vm: vm.name.clone(),
                mode,
                action: BootstrapPlanAction::WouldSkip,
                trigger: None,
                reason: "Bootstrap skipped via configuration.".to_string(),
                script_path: vm.bootstrap.script.clone(),
                payload_path: None,
                payload_bytes: None,
                handshake_timeout_secs: None,
                remote_dir: None,
                ssh: None,
                env_keys: Vec::new(),
                verify: None,
                artifact_hash: None,
                metadata_path: None,
                warnings: Vec::new(),
            };
        }
        _ => {}
    }

    let configured_payload = vm.bootstrap.payload.clone();
    let trigger = trigger_for_mode(mode);

    let script_path = match vm.bootstrap.script.as_ref() {
        Some(path) => path.clone(),
        None => {
            let reason = "Bootstrap script not configured.".to_string();
            let action = if matches!(mode, BootstrapMode::Always) {
                BootstrapPlanAction::Error
            } else {
                BootstrapPlanAction::WouldSkip
            };
            return BootstrapPlanOutcome {
                vm: vm.name.clone(),
                mode,
                action,
                trigger,
                reason,
                script_path: None,
                payload_path: None,
                payload_bytes: None,
                handshake_timeout_secs: None,
                remote_dir: None,
                ssh: None,
                env_keys: Vec::new(),
                verify: None,
                artifact_hash: None,
                metadata_path: None,
                warnings: Vec::new(),
            };
        }
    };

    if !script_path.is_file() {
        let reason = format!("Bootstrap script not found at {}.", script_path.display());
        let action = if matches!(mode, BootstrapMode::Always) {
            BootstrapPlanAction::Error
        } else {
            BootstrapPlanAction::WouldSkip
        };
        return BootstrapPlanOutcome {
            vm: vm.name.clone(),
            mode,
            action,
            trigger,
            reason,
            script_path: Some(script_path),
            payload_path: configured_payload
                .as_ref()
                .filter(|p| p.exists() && p.is_dir())
                .cloned(),
            payload_bytes: None,
            handshake_timeout_secs: None,
            remote_dir: None,
            ssh: None,
            env_keys: Vec::new(),
            verify: None,
            artifact_hash: None,
            metadata_path: None,
            warnings: Vec::new(),
        };
    }

    let inputs = match resolve_blueprint_inputs(vm, &script_path, configured_payload.as_ref()) {
        Ok(inputs) => inputs,
        Err(err) => {
            return BootstrapPlanOutcome {
                vm: vm.name.clone(),
                mode,
                action: BootstrapPlanAction::Error,
                trigger,
                reason: err,
                script_path: Some(script_path),
                payload_path: configured_payload
                    .as_ref()
                    .filter(|p| p.exists() && p.is_dir())
                    .cloned(),
                payload_bytes: None,
                handshake_timeout_secs: None,
                remote_dir: None,
                ssh: None,
                env_keys: Vec::new(),
                verify: None,
                artifact_hash: None,
                metadata_path: None,
                warnings: Vec::new(),
            };
        }
    };

    let mut env_keys: Vec<String> = inputs.env.keys().cloned().collect();
    env_keys.sort();

    let ssh_plan = Some(BootstrapPlanSsh {
        user: inputs.ssh.user.clone(),
        host: inputs.ssh.host.clone(),
        port: inputs.ssh.port,
        identity: inputs.ssh.identity.clone(),
        options: inputs.ssh.options.clone(),
    });

    let verify_plan = if inputs.verify.command.is_some() || inputs.verify.path.is_some() {
        Some(BootstrapPlanVerify {
            command: inputs.verify.command.clone(),
            path: inputs.verify.path.clone(),
            path_is_relative: inputs.verify.path_is_relative,
        })
    } else {
        None
    };

    let reason = match mode {
        BootstrapMode::Auto => {
            "Policy `auto`; pipeline runs after handshake unless the runner reports Castra:noop."
                .to_string()
        }
        BootstrapMode::Always => "Policy `always`; pipeline runs on every invocation.".to_string(),
        BootstrapMode::Skip => unreachable!(),
    };

    BootstrapPlanOutcome {
        vm: vm.name.clone(),
        mode,
        action: BootstrapPlanAction::WouldRun,
        trigger,
        reason,
        script_path: Some(script_path),
        payload_path: inputs.payload_source.clone(),
        payload_bytes: if inputs.payload_source.is_some() {
            Some(inputs.payload_bytes)
        } else {
            None
        },
        handshake_timeout_secs: Some(inputs.handshake_timeout.as_secs()),
        remote_dir: Some(inputs.remote_dir.clone()),
        ssh: ssh_plan,
        env_keys,
        verify: verify_plan,
        artifact_hash: Some(inputs.artifact_hash.clone()),
        metadata_path: inputs.metadata_path.clone(),
        warnings: inputs.warnings.clone(),
    }
}

fn trigger_for_mode(mode: BootstrapMode) -> Option<BootstrapTrigger> {
    match mode {
        BootstrapMode::Auto => Some(BootstrapTrigger::Auto),
        BootstrapMode::Always => Some(BootstrapTrigger::Always),
        BootstrapMode::Skip => None,
    }
}

fn run_for_vm(
    state_root: &Path,
    log_root: &Path,
    vm: &VmDefinition,
    emit_event: &mut dyn FnMut(Event),
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<BootstrapRunOutcome> {
    match vm.bootstrap.mode {
        BootstrapMode::Skip => {
            diagnostics.push(Diagnostic::new(
                Severity::Info,
                format!("Bootstrap skipped for VM `{}`; skipping.", vm.name),
            ));
            return Ok(BootstrapRunOutcome {
                vm: vm.name.clone(),
                status: BootstrapRunStatus::Skipped,
                stamp: None,
                log_path: None,
                ssh: None,
            });
        }
        BootstrapMode::Auto | BootstrapMode::Always => {}
    }

    let script_source = match vm.bootstrap.script.as_ref() {
        Some(path) => path.clone(),
        None => {
            let message = format!("Bootstrap script not configured for VM `{}`.", vm.name);
            diagnostics.push(
                Diagnostic::new(Severity::Info, message.clone()).with_help(format!(
                    "Add a script at `bootstrap/{}/run.sh` or set `bootstrap.script`.",
                    vm.name
                )),
            );
            if matches!(vm.bootstrap.mode, BootstrapMode::Always) {
                return Err(Error::BootstrapFailed {
                    vm: vm.name.clone(),
                    message,
                });
            }
            return Ok(BootstrapRunOutcome {
                vm: vm.name.clone(),
                status: BootstrapRunStatus::Skipped,
                stamp: None,
                log_path: None,
                ssh: None,
            });
        }
    };

    if !script_source.is_file() {
        let message = format!(
            "Bootstrap script for VM `{}` not found at {}.",
            vm.name,
            script_source.display()
        );
        diagnostics.push(
            Diagnostic::new(Severity::Info, message.clone()).with_help(format!(
                "Create the script or adjust `bootstrap.script` for VM `{}`.",
                vm.name
            )),
        );
        if matches!(vm.bootstrap.mode, BootstrapMode::Always) {
            return Err(Error::BootstrapFailed {
                vm: vm.name.clone(),
                message,
            });
        }
        return Ok(BootstrapRunOutcome {
            vm: vm.name.clone(),
            status: BootstrapRunStatus::Skipped,
            stamp: None,
            log_path: None,
            ssh: None,
        });
    }

    let payload_source = vm.bootstrap.payload.as_ref().cloned();

    let blueprint = assemble_blueprint(state_root, vm, &script_source, payload_source.as_ref())
        .map_err(|err| Error::BootstrapFailed {
            vm: vm.name.clone(),
            message: err,
        })?;

    for warning in &blueprint.warnings {
        diagnostics.push(
            Diagnostic::new(
                Severity::Warning,
                format!("Bootstrap warning for VM `{}`: {}", vm.name, warning),
            )
            .with_help("Review bootstrap configuration or metadata."),
        );
    }

    let handshake_path = state_root.join("handshakes").join(format!(
        "{}.json",
        sanitize_handshake_identity(&blueprint.handshake_identity)
    ));
    let mut context_parts = Vec::new();
    context_parts.push(format!("script {}", blueprint.script_source.display()));
    context_parts.push(format!("staged {}", blueprint.staged_script.display()));
    context_parts.push(format!("remote script {}", blueprint.remote_script));
    context_parts.push(format!("remote dir {}", blueprint.remote_dir));
    if let Some(payload_source) = blueprint.payload_source.as_ref() {
        context_parts.push(format!(
            "payload {} ({} bytes)",
            payload_source.display(),
            blueprint.payload_bytes
        ));
    } else {
        context_parts.push("payload none".to_string());
    }
    if let Some(remote_payload_dir) = blueprint.remote_payload_dir.as_ref() {
        context_parts.push(format!("remote payload dir {}", remote_payload_dir));
    }
    context_parts.push(format!(
        "handshake `{}` -> {} (timeout {}s; freshness {}s)",
        blueprint.handshake_identity,
        handshake_path.display(),
        blueprint.handshake_timeout.as_secs(),
        HANDSHAKE_FRESHNESS.as_secs()
    ));
    let mut ssh_context = Vec::new();
    ssh_context.push(format!(
        "{}@{}:{}",
        blueprint.ssh.user, blueprint.ssh.host, blueprint.ssh.port
    ));
    if let Some(identity) = blueprint.ssh.identity.as_ref() {
        ssh_context.push(format!("identity {}", identity.display()));
    }
    if !blueprint.ssh.options.is_empty() {
        ssh_context.push(format!("options {}", blueprint.ssh.options.join(", ")));
    }
    context_parts.push(format!("ssh {}", ssh_context.join("; ")));
    if !blueprint.env.is_empty() {
        let mut env_keys: Vec<_> = blueprint.env.keys().cloned().collect();
        env_keys.sort();
        context_parts.push(format!("env keys [{}]", env_keys.join(", ")));
    }
    if let Some(metadata) = blueprint.metadata_path.as_ref() {
        context_parts.push(format!("metadata {}", metadata.display()));
    }
    if let Some(command) = blueprint.verify.command.as_ref() {
        context_parts.push(format!("verify command {}", command));
    }
    if let Some(path) = blueprint.verify.path.as_ref() {
        let scope = if blueprint.verify.path_is_relative {
            "relative"
        } else {
            "absolute"
        };
        context_parts.push(format!("verify path {} ({scope})", path));
    }
    emit_event(Event::Message {
        severity: Severity::Info,
        text: format!(
            "→ {}: bootstrap context resolved: {}.",
            vm.name,
            context_parts.join("; ")
        ),
    });

    let base_hash = derive_base_hash(vm)?;
    let trigger = if matches!(vm.bootstrap.mode, BootstrapMode::Always) {
        BootstrapTrigger::Always
    } else {
        BootstrapTrigger::Auto
    };

    emit_event(Event::BootstrapStarted {
        vm: vm.name.clone(),
        base_hash: base_hash.clone(),
        artifact_hash: blueprint.artifact_hash.clone(),
        trigger,
    });

    emit_event(Event::Message {
        severity: Severity::Info,
        text: format!(
            "→ {}: waiting for broker handshake `{}` at {} (timeout {}s; freshness {}s).",
            vm.name,
            blueprint.handshake_identity,
            handshake_path.display(),
            blueprint.handshake_timeout.as_secs(),
            HANDSHAKE_FRESHNESS.as_secs()
        ),
    });

    let log_dir = log_root.join(LOG_SUBDIR);
    let mut steps = Vec::new();
    let start = Instant::now();

    let handshake_start = Instant::now();
    let handshake_result = wait_for_handshake(
        state_root,
        &vm.name,
        &blueprint.handshake_identity,
        blueprint.handshake_timeout,
    );
    let handshake_duration = handshake_start.elapsed();

    println!("FOUND HANDSHAKE");

    match handshake_result {
        Ok(handshake_ts) => {
            steps.push(StepLog::success(
                BootstrapStepKind::WaitHandshake,
                handshake_duration,
                Some(format!(
                    "Fresh handshake observed at {:?} (identity `{}` via {}).",
                    handshake_ts,
                    blueprint.handshake_identity,
                    handshake_path.display()
                )),
            ));
            emit_event(Event::BootstrapStep {
                vm: vm.name.clone(),
                step: BootstrapStepKind::WaitHandshake,
                status: BootstrapStepStatus::Success,
                duration_ms: elapsed_ms(handshake_duration),
                detail: Some(format!(
                    "Handshake fresh at {:?}; identity `{}`; file {}.",
                    handshake_ts,
                    blueprint.handshake_identity,
                    handshake_path.display()
                )),
            });
        }
        Err(err) => {
            let failure_detail = match &err {
                Error::BootstrapFailed { message, .. } => message.clone(),
                other => other.to_string(),
            };
            let enriched_detail = format!(
                "{} (identity `{}` expected at {}).",
                failure_detail,
                blueprint.handshake_identity,
                handshake_path.display()
            );
            steps.push(StepLog::from_result(
                BootstrapStepKind::WaitHandshake,
                BootstrapStepStatus::Failed,
                handshake_duration,
                Some(enriched_detail.clone()),
            ));
            emit_event(Event::BootstrapStep {
                vm: vm.name.clone(),
                step: BootstrapStepKind::WaitHandshake,
                status: BootstrapStepStatus::Failed,
                duration_ms: elapsed_ms(handshake_duration),
                detail: Some(enriched_detail.clone()),
            });
            let duration_ms = elapsed_ms(start.elapsed());
            emit_event(Event::BootstrapFailed {
                vm: vm.name.clone(),
                duration_ms,
                error: enriched_detail.clone(),
            });
            let log_record = BootstrapRunLog::failure(
                &vm.name,
                &blueprint,
                &base_hash,
                steps,
                duration_ms,
                enriched_detail.clone(),
            );
            write_run_log(&log_dir, &log_record).map_err(|io_err| Error::BootstrapFailed {
                vm: vm.name.clone(),
                message: format!("Failed to persist bootstrap log: {io_err}"),
            })?;
            return Err(err);
        }
    }

    let mut connectivity_context = Vec::new();
    connectivity_context.push(format!(
        "{}@{}:{}",
        blueprint.ssh.user, blueprint.ssh.host, blueprint.ssh.port
    ));
    if let Some(identity) = blueprint.ssh.identity.as_ref() {
        connectivity_context.push(format!("identity {}", identity.display()));
    }
    if !blueprint.ssh.options.is_empty() {
        connectivity_context.push(format!("options {}", blueprint.ssh.options.join(", ")));
    }
    emit_event(Event::Message {
        severity: Severity::Info,
        text: format!(
            "→ {}: checking SSH connectivity ({}).",
            vm.name,
            connectivity_context.join("; ")
        ),
    });

    println!("CHECKING CONNECTIVITY");
    let connect_outcome = check_connectivity(&blueprint);
    let connect_duration = connect_outcome.duration;
    emit_event(Event::BootstrapStep {
        vm: vm.name.clone(),
        step: BootstrapStepKind::Connect,
        status: connect_outcome.status,
        duration_ms: elapsed_ms(connect_duration),
        detail: connect_outcome.detail.clone(),
    });
    steps.push(StepLog::from_result(
        BootstrapStepKind::Connect,
        connect_outcome.status,
        connect_duration,
        connect_outcome.detail.clone(),
    ));
    if !matches!(connect_outcome.status, BootstrapStepStatus::Success) {
        let duration_ms = elapsed_ms(start.elapsed());
        let failure_detail = connect_outcome
            .detail
            .clone()
            .unwrap_or_else(|| "Failed to establish SSH connectivity.".to_string());
        emit_event(Event::BootstrapFailed {
            vm: vm.name.clone(),
            duration_ms,
            error: failure_detail.clone(),
        });
        let log_record = BootstrapRunLog::failure(
            &vm.name,
            &blueprint,
            &base_hash,
            steps,
            duration_ms,
            failure_detail.clone(),
        );
        write_run_log(&log_dir, &log_record).map_err(|io_err| Error::BootstrapFailed {
            vm: vm.name.clone(),
            message: format!("Failed to persist bootstrap log: {io_err}"),
        })?;

        println!("BOOTSTRAP FAILED");
        return Err(Error::BootstrapFailed {
            vm: vm.name.clone(),
            message: failure_detail,
        });
    }

    println!("TRANSFERRING CONTEXT");

    let mut transfer_context = Vec::new();
    transfer_context.push(format!(
        "{} -> {}",
        blueprint.staged_script.display(),
        blueprint.remote_script
    ));
    if let Some(staged_payload) = blueprint.staged_payload.as_ref() {
        let remote_payload = blueprint
            .remote_payload_dir
            .as_deref()
            .unwrap_or(&blueprint.remote_dir);
        transfer_context.push(format!(
            "{} -> {} ({} bytes)",
            staged_payload.display(),
            remote_payload,
            blueprint.payload_bytes
        ));
    } else {
        transfer_context.push("no payload staging".to_string());
    }
    emit_event(Event::Message {
        severity: Severity::Info,
        text: format!(
            "→ {}: transferring bootstrap assets: {}.",
            vm.name,
            transfer_context.join("; ")
        ),
    });

    let transfer_outcome = transfer_artifacts(&blueprint);
    let transfer_duration = transfer_outcome.duration;
    emit_event(Event::BootstrapStep {
        vm: vm.name.clone(),
        step: BootstrapStepKind::Transfer,
        status: transfer_outcome.status,
        duration_ms: elapsed_ms(transfer_duration),
        detail: transfer_outcome.detail.clone(),
    });
    steps.push(StepLog::from_result(
        BootstrapStepKind::Transfer,
        transfer_outcome.status,
        transfer_duration,
        transfer_outcome.detail.clone(),
    ));
    if !matches!(transfer_outcome.status, BootstrapStepStatus::Success) {
        let duration_ms = elapsed_ms(start.elapsed());
        let failure_detail = transfer_outcome
            .detail
            .clone()
            .unwrap_or_else(|| "Failed to transfer bootstrap artifacts.".to_string());
        emit_event(Event::BootstrapFailed {
            vm: vm.name.clone(),
            duration_ms,
            error: failure_detail.clone(),
        });
        let log_record = BootstrapRunLog::failure(
            &vm.name,
            &blueprint,
            &base_hash,
            steps,
            duration_ms,
            failure_detail.clone(),
        );
        write_run_log(&log_dir, &log_record).map_err(|io_err| Error::BootstrapFailed {
            vm: vm.name.clone(),
            message: format!("Failed to persist bootstrap log: {io_err}"),
        })?;
        return Err(Error::BootstrapFailed {
            vm: vm.name.clone(),
            message: failure_detail,
        });
    }

    println!("APPLYING CONTEXT");

    let mut apply_context = Vec::new();
    apply_context.push(format!("remote dir {}", blueprint.remote_dir));
    apply_context.push(format!("script {}", blueprint.remote_script));
    if let Some(remote_payload_dir) = blueprint.remote_payload_dir.as_ref() {
        apply_context.push(format!("payload dir {}", remote_payload_dir));
    }
    if !blueprint.env.is_empty() {
        let mut env_keys: Vec<_> = blueprint.env.keys().cloned().collect();
        env_keys.sort();
        apply_context.push(format!("env keys [{}]", env_keys.join(", ")));
    } else {
        apply_context.push("env keys []".to_string());
    }
    emit_event(Event::Message {
        severity: Severity::Info,
        text: format!(
            "→ {}: executing remote bootstrap script ({})",
            vm.name,
            apply_context.join("; ")
        ),
    });

    let apply_outcome = execute_remote(&blueprint);
    let apply_duration = apply_outcome.command.duration;
    emit_event(Event::BootstrapStep {
        vm: vm.name.clone(),
        step: BootstrapStepKind::Apply,
        status: apply_outcome.command.status,
        duration_ms: elapsed_ms(apply_duration),
        detail: apply_outcome.command.detail.clone(),
    });
    steps.push(StepLog::from_result(
        BootstrapStepKind::Apply,
        apply_outcome.command.status,
        apply_duration,
        apply_outcome.command.detail.clone(),
    ));
    if !matches!(apply_outcome.command.status, BootstrapStepStatus::Success) {
        let duration_ms = elapsed_ms(start.elapsed());
        let failure_detail = apply_outcome
            .command
            .detail
            .clone()
            .unwrap_or_else(|| "Remote bootstrap execution failed.".to_string());
        emit_event(Event::BootstrapFailed {
            vm: vm.name.clone(),
            duration_ms,
            error: failure_detail.clone(),
        });
        let log_record = BootstrapRunLog::failure(
            &vm.name,
            &blueprint,
            &base_hash,
            steps,
            duration_ms,
            failure_detail.clone(),
        );
        write_run_log(&log_dir, &log_record).map_err(|io_err| Error::BootstrapFailed {
            vm: vm.name.clone(),
            message: format!("Failed to persist bootstrap log: {io_err}"),
        })?;
        return Err(Error::BootstrapFailed {
            vm: vm.name.clone(),
            message: failure_detail,
        });
    }

    println!("VERIFYING CONTEXT");

    let mut verify_context = Vec::new();
    if let Some(command) = blueprint.verify.command.as_ref() {
        verify_context.push(format!("command {}", command));
    }
    if let Some(path) = blueprint.verify.path.as_ref() {
        let scope = if blueprint.verify.path_is_relative {
            "relative"
        } else {
            "absolute"
        };
        verify_context.push(format!("path {} ({scope})", path));
    }
    if verify_context.is_empty() {
        verify_context.push("no verification checks configured".to_string());
    }
    emit_event(Event::Message {
        severity: Severity::Info,
        text: format!(
            "→ {}: verifying remote state ({})",
            vm.name,
            verify_context.join("; ")
        ),
    });

    let verify_outcome = verify_remote(&blueprint);
    let verify_duration = verify_outcome.duration;
    emit_event(Event::BootstrapStep {
        vm: vm.name.clone(),
        step: BootstrapStepKind::Verify,
        status: verify_outcome.status,
        duration_ms: elapsed_ms(verify_duration),
        detail: verify_outcome.detail.clone(),
    });
    steps.push(StepLog::from_result(
        BootstrapStepKind::Verify,
        verify_outcome.status,
        verify_duration,
        verify_outcome.detail.clone(),
    ));
    if !matches!(verify_outcome.status, BootstrapStepStatus::Success) {
        let duration_ms = elapsed_ms(start.elapsed());
        let failure_detail = verify_outcome
            .detail
            .clone()
            .unwrap_or_else(|| "Bootstrap verification failed.".to_string());
        emit_event(Event::BootstrapFailed {
            vm: vm.name.clone(),
            duration_ms,
            error: failure_detail.clone(),
        });
        let log_record = BootstrapRunLog::failure(
            &vm.name,
            &blueprint,
            &base_hash,
            steps,
            duration_ms,
            failure_detail.clone(),
        );
        write_run_log(&log_dir, &log_record).map_err(|io_err| Error::BootstrapFailed {
            vm: vm.name.clone(),
            message: format!("Failed to persist bootstrap log: {io_err}"),
        })?;
        return Err(Error::BootstrapFailed {
            vm: vm.name.clone(),
            message: failure_detail,
        });
    }

    let total_ms = elapsed_ms(start.elapsed());
    let final_status = match apply_outcome.completion {
        ApplyCompletion::NoOp => BootstrapStatus::NoOp,
        ApplyCompletion::Success => BootstrapStatus::Success,
    };

    emit_event(Event::BootstrapCompleted {
        vm: vm.name.clone(),
        status: final_status,
        duration_ms: total_ms,
        stamp: None,
    });

    let log_record = BootstrapRunLog::success(
        &vm.name,
        &blueprint,
        &base_hash,
        steps,
        total_ms,
        final_status,
    );
    let log_path =
        write_run_log(&log_dir, &log_record).map_err(|io_err| Error::BootstrapFailed {
            vm: vm.name.clone(),
            message: format!("Failed to persist bootstrap log: {io_err}"),
        })?;

    let run_status = match final_status {
        BootstrapStatus::NoOp => BootstrapRunStatus::NoOp,
        BootstrapStatus::Success => BootstrapRunStatus::Success,
    };

    Ok(BootstrapRunOutcome {
        vm: vm.name.clone(),
        status: run_status,
        stamp: None,
        log_path: Some(log_path),
        ssh: Some(plan_ssh_from_config(&blueprint.ssh)),
    })
}

fn wait_for_handshake(
    state_root: &Path,
    vm: &str,
    handshake_identity: &str,
    timeout: Duration,
) -> Result<SystemTime> {
    let file_name = format!("{}.json", sanitize_handshake_identity(handshake_identity));
    let handshake_path = state_root.join("handshakes").join(file_name);
    let deadline = Instant::now() + timeout;

    println!("WAITING FOR HANDSHAKE");

    loop {
        if let Some(timestamp) = read_handshake_timestamp(&handshake_path)? {
            println!("TIMESTAMP: {:?}", timestamp);
            let now = SystemTime::now();
            if now
                .duration_since(timestamp)
                .unwrap_or_else(|_| Duration::from_secs(0))
                <= HANDSHAKE_FRESHNESS
            {
                return Ok(timestamp);
            }
        }

        let now = Instant::now();
        if now >= deadline {
            return Err(Error::BootstrapFailed {
                vm: vm.to_string(),
                message: format!(
                    "Timed out waiting for fresh broker handshake `{}` at {} after {} seconds (freshness window {}s).",
                    handshake_identity,
                    handshake_path.display(),
                    timeout.as_secs(),
                    HANDSHAKE_FRESHNESS.as_secs()
                ),
            });
        }

        let remaining = deadline.saturating_duration_since(now);
        let sleep_for = remaining.min(Duration::from_millis(500));
        if sleep_for.is_zero() {
            std::thread::yield_now();
        } else {
            std::thread::sleep(sleep_for);
        }
    }
}

fn read_handshake_timestamp(path: &Path) -> Result<Option<SystemTime>> {
    let contents = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(err) => {
            return Err(Error::BootstrapFailed {
                vm: path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("<unknown>")
                    .to_string(),
                message: format!("Failed to read handshake file {}: {err}", path.display()),
            });
        }
    };

    #[derive(Deserialize)]
    struct HandshakeFile {
        timestamp: u64,
    }

    let parsed: HandshakeFile =
        serde_json::from_slice(&contents).map_err(|err| Error::BootstrapFailed {
            vm: path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("<unknown>")
                .to_string(),
            message: format!("Malformed handshake file {}: {err}", path.display()),
        })?;

    Ok(Some(UNIX_EPOCH + Duration::from_secs(parsed.timestamp)))
}

fn sanitize_handshake_identity(identity: &str) -> String {
    let mut sanitized: String = identity
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '_'
            }
        })
        .collect();
    if sanitized.chars().all(|ch| ch == '_' || ch == '.') {
        sanitized = "vm".to_string();
    }
    sanitized
}

fn derive_base_hash(vm: &VmDefinition) -> Result<String> {
    compute_file_sha256(vm.base_image.path()).map_err(|err| Error::BootstrapFailed {
        vm: vm.name.clone(),
        message: err,
    })
}

fn compute_file_sha256(path: &Path) -> std::result::Result<String, String> {
    let mut file = File::open(path).map_err(|err| {
        format!(
            "Failed to open base image {} for hashing: {err}",
            path.display()
        )
    })?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 131_072];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|err| format!("Error hashing {}: {err}", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex::encode(hasher.finalize()))
}

#[derive(Debug)]
struct BootstrapBlueprintInputs {
    vm: String,
    handshake_identity: String,
    script_source: PathBuf,
    payload_source: Option<PathBuf>,
    payload_bytes: u64,
    handshake_timeout: Duration,
    remote_dir: String,
    remote_script: String,
    remote_payload_dir: Option<String>,
    ssh: SshConfig,
    env: HashMap<String, String>,
    verify: BootstrapVerifyPlan,
    artifact_hash: String,
    metadata_path: Option<PathBuf>,
    warnings: Vec<String>,
}

#[derive(Debug)]
struct BootstrapBlueprint {
    vm: String,
    handshake_identity: String,
    script_source: PathBuf,
    staged_script: PathBuf,
    payload_source: Option<PathBuf>,
    staged_payload: Option<PathBuf>,
    payload_bytes: u64,
    handshake_timeout: Duration,
    remote_dir: String,
    remote_script: String,
    remote_payload_dir: Option<String>,
    ssh: SshConfig,
    env: HashMap<String, String>,
    verify: BootstrapVerifyPlan,
    artifact_hash: String,
    metadata_path: Option<PathBuf>,
    warnings: Vec<String>,
}

#[derive(Debug, Clone)]
struct SshConfig {
    user: String,
    host: String,
    port: u16,
    identity: Option<PathBuf>,
    options: Vec<String>,
}

fn plan_ssh_from_config(ssh: &SshConfig) -> BootstrapPlanSsh {
    BootstrapPlanSsh {
        user: ssh.user.clone(),
        host: ssh.host.clone(),
        port: ssh.port,
        identity: ssh.identity.clone(),
        options: ssh.options.clone(),
    }
}

#[derive(Debug, Clone)]
struct BootstrapVerifyPlan {
    command: Option<String>,
    path: Option<String>,
    path_is_relative: bool,
}

#[derive(Default, Deserialize)]
struct BlueprintMetadata {
    #[serde(default)]
    ssh: Option<MetadataSsh>,
    #[serde(default)]
    verify: Option<MetadataVerify>,
    #[serde(default)]
    handshake_identity: Option<String>,
    #[serde(default)]
    handshake_timeout_secs: Option<u64>,
    #[serde(default)]
    remote_dir: Option<String>,
    #[serde(default)]
    env: HashMap<String, String>,
}

#[derive(Deserialize)]
struct MetadataSsh {
    #[serde(default)]
    user: Option<String>,
    #[serde(default)]
    host: Option<String>,
    #[serde(default)]
    port: Option<u16>,
    #[serde(default)]
    identity: Option<String>,
    #[serde(default)]
    options: Vec<String>,
}

#[derive(Default, Deserialize)]
struct MetadataVerify {
    #[serde(default)]
    command: Option<String>,
    #[serde(default)]
    path: Option<String>,
}

fn assemble_blueprint(
    state_root: &Path,
    vm: &VmDefinition,
    script_source: &Path,
    payload_source: Option<&PathBuf>,
) -> std::result::Result<BootstrapBlueprint, String> {
    let inputs = resolve_blueprint_inputs(vm, script_source, payload_source)?;
    let staging_root = state_root.join(STAGING_SUBDIR).join(&vm.name);
    let (staged_script, staged_payload, payload_bytes) = stage_local_assets(
        &inputs.script_source,
        inputs.payload_source.as_deref(),
        &staging_root,
    )?;

    let BootstrapBlueprintInputs {
        vm,
        handshake_identity,
        script_source: resolved_script,
        payload_source,
        payload_bytes: _,
        handshake_timeout,
        remote_dir,
        remote_script,
        remote_payload_dir,
        ssh,
        env,
        verify,
        artifact_hash,
        metadata_path,
        warnings,
    } = inputs;

    Ok(BootstrapBlueprint {
        vm,
        script_source: resolved_script,
        handshake_identity,
        staged_script,
        payload_source,
        staged_payload,
        payload_bytes,
        handshake_timeout,
        remote_dir,
        remote_script,
        remote_payload_dir,
        ssh,
        env,
        verify,
        artifact_hash,
        metadata_path,
        warnings,
    })
}

fn resolve_blueprint_inputs(
    vm: &VmDefinition,
    script_source: &Path,
    payload_source: Option<&PathBuf>,
) -> std::result::Result<BootstrapBlueprintInputs, String> {
    let mut warnings = Vec::new();

    let metadata_path = script_source
        .parent()
        .map(|parent| parent.join("bootstrap.toml"))
        .filter(|path| path.is_file());

    let metadata = match metadata_path.as_ref() {
        Some(path) => Some(load_metadata(path)?),
        None => None,
    };

    let mut handshake_secs = vm.bootstrap.handshake_timeout_secs;
    let mut handshake_identity = vm.name.clone();
    if let Some(meta) = metadata.as_ref() {
        if let Some(value) = meta.handshake_timeout_secs {
            if value == 0 {
                return Err(format!(
                    "bootstrap.toml for `{}` specifies handshake_timeout_secs = 0; specify at least 1 second.",
                    vm.name
                ));
            }
            handshake_secs = value;
        }
        if let Some(identity) = meta.handshake_identity.as_ref() {
            let trimmed = identity.trim();
            if trimmed.is_empty() {
                return Err(format!(
                    "bootstrap.toml for `{}` specifies an empty handshake_identity.",
                    vm.name
                ));
            }
            handshake_identity = trimmed.to_string();
        }
    }
    let handshake_timeout = Duration::from_secs(handshake_secs);

    let mut remote_dir = vm.bootstrap.remote_dir.to_string_lossy().into_owned();
    if remote_dir.trim().is_empty() {
        return Err(format!(
            "VM `{}` resolved an empty bootstrap remote_dir.",
            vm.name
        ));
    }

    if remote_dir == DEFAULT_REMOTE_BASE {
        remote_dir = format!("{}/{}", remote_dir.trim_end_matches('/'), vm.name);
    }

    if let Some(meta) = metadata.as_ref() {
        if let Some(dir) = meta.remote_dir.as_ref() {
            if dir.trim().is_empty() {
                return Err(format!(
                    "bootstrap.toml for `{}` specifies an empty remote_dir.",
                    vm.name
                ));
            }
            remote_dir = dir.clone();
        }
    }

    remote_dir = normalize_remote_dir(&remote_dir);
    let remote_script = format!("{remote_dir}/{}", STAGED_SCRIPT_NAME);

    let payload_source_resolved = match payload_source {
        Some(path) if path.exists() => {
            if path.is_dir() {
                Some(path.clone())
            } else {
                return Err(format!(
                    "Payload path {} for VM `{}` is not a directory.",
                    path.display(),
                    vm.name
                ));
            }
        }
        Some(path) => {
            warnings.push(format!(
                "Payload directory not found at {}; continuing without payload.",
                path.display()
            ));
            None
        }
        None => None,
    };

    let remote_payload_dir = payload_source_resolved
        .as_ref()
        .map(|_| format!("{remote_dir}/{}", STAGED_PAYLOAD_DIR));

    let payload_bytes = match payload_source_resolved.as_ref() {
        Some(path) => calculate_payload_bytes(path)?,
        None => 0,
    };

    let mut env = metadata
        .as_ref()
        .map(|meta| meta.env.clone())
        .unwrap_or_default();
    for (key, value) in &vm.bootstrap.env {
        env.insert(key.clone(), value.clone());
    }

    let mut verify_command = metadata
        .as_ref()
        .and_then(|meta| meta.verify.as_ref())
        .and_then(|verify| verify.command.clone());
    let mut verify_path = metadata
        .as_ref()
        .and_then(|meta| meta.verify.as_ref())
        .and_then(|verify| verify.path.clone());
    let mut verify_path_is_relative = verify_path
        .as_ref()
        .map(|path| !path.starts_with('/'))
        .unwrap_or(false);

    if let Some(config_verify) = &vm.bootstrap.verify {
        if let Some(cmd) = config_verify.command.as_ref() {
            verify_command = Some(cmd.clone());
        }
        if let Some(path) = config_verify.path.as_ref() {
            verify_path_is_relative = !path.is_absolute();
            verify_path = Some(path.to_string_lossy().into_owned());
        }
    }

    let verify = BootstrapVerifyPlan {
        command: verify_command,
        path: verify_path,
        path_is_relative: verify_path_is_relative,
    };

    let mut ssh = SshConfig {
        user: DEFAULT_SSH_USER.to_string(),
        host: DEFAULT_SSH_HOST.to_string(),
        port: DEFAULT_SSH_PORT,
        identity: None,
        options: Vec::new(),
    };

    if let Some(meta_ssh) = metadata.as_ref().and_then(|meta| meta.ssh.as_ref()) {
        if let Some(user) = meta_ssh.user.as_ref() {
            if !user.trim().is_empty() {
                ssh.user = user.clone();
            }
        }
        if let Some(host) = meta_ssh.host.as_ref() {
            if !host.trim().is_empty() {
                ssh.host = host.clone();
            }
        }
        if let Some(port) = meta_ssh.port {
            if port == 0 {
                return Err(format!(
                    "bootstrap.toml for `{}` specifies ssh.port = 0; specify a non-zero port.",
                    vm.name
                ));
            }
            ssh.port = port;
        }
        if let Some(identity) = meta_ssh.identity.as_ref() {
            if !identity.trim().is_empty() {
                let resolved = metadata_path
                    .as_ref()
                    .and_then(|path| path.parent())
                    .map(|dir| dir.join(identity))
                    .unwrap_or_else(|| PathBuf::from(identity));
                ssh.identity = Some(resolved);
            }
        }
        if !meta_ssh.options.is_empty() {
            ssh.options = meta_ssh.options.clone();
        }
    }

    ensure_default_ssh_options(&mut ssh.options);

    if ssh.port == DEFAULT_SSH_PORT && ssh.host == DEFAULT_SSH_HOST {
        if let Some(forward) = vm
            .port_forwards
            .iter()
            .find(|pf| pf.protocol == PortProtocol::Tcp && pf.guest == 22)
        {
            ssh.port = forward.host;
        } else {
            warnings.push(format!(
                "Using fallback SSH port {}; no TCP port forward to guest 22 declared.",
                DEFAULT_SSH_PORT
            ));
        }
    }

    let artifact_hash = compute_artifact_hash(
        script_source,
        payload_source_resolved.as_deref(),
        &env,
        &remote_dir,
        &verify,
    )?;

    Ok(BootstrapBlueprintInputs {
        vm: vm.name.clone(),
        handshake_identity,
        script_source: script_source.to_path_buf(),
        payload_source: payload_source_resolved,
        payload_bytes,
        handshake_timeout,
        remote_dir,
        remote_script,
        remote_payload_dir,
        ssh,
        env,
        verify,
        artifact_hash,
        metadata_path,
        warnings,
    })
}

fn load_metadata(path: &Path) -> std::result::Result<BlueprintMetadata, String> {
    let bytes = fs::read(path).map_err(|err| {
        format!(
            "Failed to read bootstrap metadata {}: {err}",
            path.display()
        )
    })?;
    let contents = String::from_utf8(bytes).map_err(|err| {
        format!(
            "Bootstrap metadata at {} is not valid UTF-8: {err}",
            path.display()
        )
    })?;
    toml::from_str(&contents).map_err(|err| {
        format!(
            "Failed to parse bootstrap metadata {}: {err}",
            path.display()
        )
    })
}

fn normalize_remote_dir(input: &str) -> String {
    if input == "/" {
        "/".to_string()
    } else {
        input.trim_end_matches('/').to_string()
    }
}

fn stage_local_assets(
    script_source: &Path,
    payload_source: Option<&Path>,
    staging_root: &Path,
) -> std::result::Result<(PathBuf, Option<PathBuf>, u64), String> {
    if staging_root.exists() {
        fs::remove_dir_all(staging_root).map_err(|err| {
            format!(
                "Failed to clear bootstrap staging directory {}: {err}",
                staging_root.display()
            )
        })?;
    }
    fs::create_dir_all(staging_root).map_err(|err| {
        format!(
            "Failed to create bootstrap staging directory {}: {err}",
            staging_root.display()
        )
    })?;

    let staged_script = staging_root.join(STAGED_SCRIPT_NAME);
    if let Some(parent) = staged_script.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            format!(
                "Failed to create staging directory {}: {err}",
                parent.display()
            )
        })?;
    }
    fs::copy(script_source, &staged_script).map_err(|err| {
        format!(
            "Failed to copy bootstrap script from {} to {}: {err}",
            script_source.display(),
            staged_script.display()
        )
    })?;
    if let Ok(metadata) = fs::metadata(script_source) {
        if let Err(err) = fs::set_permissions(&staged_script, metadata.permissions()) {
            return Err(format!(
                "Failed to set permissions on staged script {}: {err}",
                staged_script.display()
            ));
        }
    }

    let mut payload_bytes = 0;
    let staged_payload = if let Some(source) = payload_source {
        let dest = staging_root.join(STAGED_PAYLOAD_DIR);
        fs::create_dir_all(&dest).map_err(|err| {
            format!(
                "Failed to create staged payload directory {}: {err}",
                dest.display()
            )
        })?;
        payload_bytes = copy_payload_dir(source, &dest)?;
        Some(dest)
    } else {
        None
    };

    Ok((staged_script, staged_payload, payload_bytes))
}

fn copy_payload_dir(source: &Path, dest: &Path) -> std::result::Result<u64, String> {
    let mut stack = vec![source.to_path_buf()];
    let mut total = 0u64;

    while let Some(current) = stack.pop() {
        let metadata = fs::symlink_metadata(&current).map_err(|err| {
            format!(
                "Failed to inspect payload entry {}: {err}",
                current.display()
            )
        })?;

        let rel = current
            .strip_prefix(source)
            .map_err(|_| format!("Failed to compute relative path for {}", current.display()))?;
        let target = dest.join(rel);

        if metadata.is_dir() {
            fs::create_dir_all(&target).map_err(|err| {
                format!(
                    "Failed to create payload directory {}: {err}",
                    target.display()
                )
            })?;
            for entry in fs::read_dir(&current).map_err(|err| {
                format!(
                    "Failed to read payload directory {}: {err}",
                    current.display()
                )
            })? {
                let entry = entry.map_err(|err| {
                    format!(
                        "Failed to read payload entry in {}: {err}",
                        current.display()
                    )
                })?;
                stack.push(entry.path());
            }
        } else if metadata.is_file() {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).map_err(|err| {
                    format!(
                        "Failed to create payload parent directory {}: {err}",
                        parent.display()
                    )
                })?;
            }
            fs::copy(&current, &target).map_err(|err| {
                format!("Failed to copy payload file {}: {err}", current.display())
            })?;
            fs::set_permissions(&target, metadata.permissions()).map_err(|err| {
                format!(
                    "Failed to set permissions on staged payload file {}: {err}",
                    target.display()
                )
            })?;
            total = total.saturating_add(metadata.len());
        } else {
            return Err(format!(
                "Unsupported payload entry type at {}; only files and directories are supported.",
                current.display()
            ));
        }
    }

    Ok(total)
}

fn calculate_payload_bytes(root: &Path) -> std::result::Result<u64, String> {
    let entries = collect_payload_entries(root)?;
    Ok(entries
        .iter()
        .filter_map(|entry| {
            if matches!(entry.kind, PayloadEntryKind::File) {
                Some(entry.size)
            } else {
                None
            }
        })
        .sum())
}

#[derive(Debug)]
struct PayloadEntry {
    rel_path: String,
    source_path: PathBuf,
    kind: PayloadEntryKind,
    size: u64,
}

#[derive(Debug, Clone, Copy)]
enum PayloadEntryKind {
    File,
    Directory,
}

fn collect_payload_entries(root: &Path) -> std::result::Result<Vec<PayloadEntry>, String> {
    let mut entries = Vec::new();
    let mut stack = vec![root.to_path_buf()];

    while let Some(current) = stack.pop() {
        let metadata = fs::symlink_metadata(&current).map_err(|err| {
            format!(
                "Failed to inspect staged payload entry {}: {err}",
                current.display()
            )
        })?;

        if current != root {
            let rel = current.strip_prefix(root).map_err(|_| {
                format!("Failed to compute relative path for {}", current.display())
            })?;
            let rel_path = normalize_relative_path(rel);
            if metadata.is_dir() {
                entries.push(PayloadEntry {
                    rel_path,
                    source_path: current.clone(),
                    kind: PayloadEntryKind::Directory,
                    size: 0,
                });
            } else if metadata.is_file() {
                entries.push(PayloadEntry {
                    rel_path,
                    source_path: current.clone(),
                    kind: PayloadEntryKind::File,
                    size: metadata.len(),
                });
            } else {
                return Err(format!(
                    "Unsupported staged payload entry at {}; only files and directories are supported.",
                    current.display()
                ));
            }
        }

        if metadata.is_dir() {
            for entry in fs::read_dir(&current).map_err(|err| {
                format!(
                    "Failed to read staged payload directory {}: {err}",
                    current.display()
                )
            })? {
                let entry = entry.map_err(|err| {
                    format!(
                        "Failed to read staged payload entry in {}: {err}",
                        current.display()
                    )
                })?;
                stack.push(entry.path());
            }
        }
    }

    entries.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));
    Ok(entries)
}

fn normalize_relative_path(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

fn compute_artifact_hash(
    script: &Path,
    payload: Option<&Path>,
    env: &HashMap<String, String>,
    remote_dir: &str,
    verify: &BootstrapVerifyPlan,
) -> std::result::Result<String, String> {
    let mut hasher = Sha256::new();

    hasher.update(b"script\0");
    let mut file = File::open(script)
        .map_err(|err| format!("Failed to read staged script {}: {err}", script.display()))?;
    let mut buffer = [0u8; 131_072];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|err| format!("Failed to hash staged script {}: {err}", script.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    hasher.update(b"\0");

    hasher.update(b"payload\0");
    if let Some(payload_path) = payload {
        let entries = collect_payload_entries(payload_path)?;
        for entry in entries {
            match entry.kind {
                PayloadEntryKind::Directory => {
                    hasher.update(b"dir\0");
                    hasher.update(entry.rel_path.as_bytes());
                    hasher.update(b"\0");
                }
                PayloadEntryKind::File => {
                    hasher.update(b"file\0");
                    hasher.update(entry.rel_path.as_bytes());
                    hasher.update(b"\0");
                    let mut file = File::open(&entry.source_path).map_err(|err| {
                        format!(
                            "Failed to read staged payload file {}: {err}",
                            entry.source_path.display()
                        )
                    })?;
                    loop {
                        let read = file.read(&mut buffer).map_err(|err| {
                            format!(
                                "Failed to hash staged payload file {}: {err}",
                                entry.source_path.display()
                            )
                        })?;
                        if read == 0 {
                            break;
                        }
                        hasher.update(&buffer[..read]);
                    }
                }
            }
        }
    }
    hasher.update(b"\0");

    hasher.update(b"env\0");
    let mut env_pairs: Vec<_> = env.iter().collect();
    env_pairs.sort_by(|a, b| a.0.cmp(b.0));
    for (key, value) in env_pairs {
        hasher.update(key.as_bytes());
        hasher.update(b"\0");
        hasher.update(value.as_bytes());
        hasher.update(b"\0");
    }
    hasher.update(b"\0");

    hasher.update(b"remote_dir\0");
    hasher.update(remote_dir.as_bytes());
    hasher.update(b"\0");

    hasher.update(b"verify_command\0");
    if let Some(cmd) = verify.command.as_ref() {
        hasher.update(cmd.as_bytes());
    }
    hasher.update(b"\0verify_path\0");
    if let Some(path) = verify.path.as_ref() {
        let path_repr = if verify.path_is_relative {
            format!("{}/{}", remote_dir, path)
        } else {
            path.clone()
        };
        hasher.update(path_repr.as_bytes());
    }
    hasher.update(b"\0");

    Ok(hex::encode(hasher.finalize()))
}

fn ensure_default_ssh_options(options: &mut Vec<String>) {
    for default in DEFAULT_SSH_OPTIONS {
        if !options.iter().any(|opt| opt == default) {
            options.push(default.to_string());
        }
    }
}

fn generate_run_id() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_secs(0));
    format!("{:x}{:x}", now.as_secs(), now.subsec_nanos())
}

struct StepLog {
    kind: BootstrapStepKind,
    status: BootstrapStepStatus,
    duration_ms: u64,
    detail: Option<String>,
}

impl StepLog {
    fn success(kind: BootstrapStepKind, duration: Duration, detail: Option<String>) -> Self {
        Self {
            kind,
            status: BootstrapStepStatus::Success,
            duration_ms: elapsed_ms(duration),
            detail,
        }
    }

    fn from_result(
        kind: BootstrapStepKind,
        status: BootstrapStepStatus,
        duration: Duration,
        detail: Option<String>,
    ) -> Self {
        Self {
            kind,
            status,
            duration_ms: elapsed_ms(duration),
            detail,
        }
    }
}

struct CommandOutcome {
    status: BootstrapStepStatus,
    duration: Duration,
    detail: Option<String>,
}

struct ApplyOutcome {
    command: CommandOutcome,
    completion: ApplyCompletion,
}

enum ApplyCompletion {
    Success,
    NoOp,
}

struct ProcessOutput {
    command: String,
    stdout: String,
    stderr: String,
}

fn format_cli(program: &str, args: &[String]) -> String {
    let mut parts = Vec::with_capacity(args.len() + 1);
    parts.push(program.to_string());
    for arg in args {
        if arg.chars().all(|ch| {
            ch.is_ascii_alphanumeric()
                || matches!(ch, '-' | '_' | '/' | '.' | ':' | '=' | ',' | '@')
        }) {
            parts.push(arg.clone());
        } else {
            parts.push(shell_quote(arg));
        }
    }
    parts.join(" ")
}

fn truncate_for_log(input: &str, limit: usize) -> String {
    let mut buffer = String::new();
    let mut chars = input.chars();
    for _ in 0..limit {
        match chars.next() {
            Some(ch) => buffer.push(ch),
            None => return buffer,
        }
    }
    if chars.next().is_some() {
        buffer.push_str("...");
    }
    buffer
}

fn summarize_output(label: &str, contents: &str) -> Option<String> {
    let trimmed = contents.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(format!("{label}: {}", truncate_for_log(trimmed, 240)))
    }
}

fn append_command_detail(parts: &mut Vec<String>, label: &str, output: &ProcessOutput) {
    parts.push(format!("{label} via `{}`.", output.command));
    if let Some(snippet) = summarize_output("stdout", &output.stdout) {
        parts.push(snippet);
    }
    if let Some(snippet) = summarize_output("stderr", &output.stderr) {
        parts.push(snippet);
    }
}

fn check_connectivity(blueprint: &BootstrapBlueprint) -> CommandOutcome {
    let start = Instant::now();
    let probe_args = vec![String::from("true")];
    match run_ssh_command_capture(&blueprint.ssh, &probe_args) {
        Ok(output) => {
            let mut detail_parts = vec![format!(
                "SSH connectivity confirmed ({}@{}:{}) via `{}`.",
                blueprint.ssh.user, blueprint.ssh.host, blueprint.ssh.port, output.command
            )];
            if let Some(identity) = blueprint.ssh.identity.as_ref() {
                detail_parts.push(format!("identity {}", identity.display()));
            }
            if !blueprint.ssh.options.is_empty() {
                detail_parts.push(format!("options {}", blueprint.ssh.options.join(", ")));
            }
            if let Some(snippet) = summarize_output("stdout", &output.stdout) {
                detail_parts.push(snippet);
            }
            if let Some(snippet) = summarize_output("stderr", &output.stderr) {
                detail_parts.push(snippet);
            }
            CommandOutcome {
                status: BootstrapStepStatus::Success,
                duration: start.elapsed(),
                detail: Some(detail_parts.join(" ")),
            }
        }
        Err(err) => CommandOutcome {
            status: BootstrapStepStatus::Failed,
            duration: start.elapsed(),
            detail: Some(err),
        },
    }
}

fn transfer_artifacts(blueprint: &BootstrapBlueprint) -> CommandOutcome {
    let start = Instant::now();

    let mut detail_parts = Vec::new();

    match prepare_remote_directory(blueprint) {
        Ok(output) => {
            append_command_detail(&mut detail_parts, "Prepared remote directory", &output)
        }
        Err(err) => {
            return CommandOutcome {
                status: BootstrapStepStatus::Failed,
                duration: start.elapsed(),
                detail: Some(err),
            };
        }
    }

    match run_scp_path(
        &blueprint.ssh,
        &blueprint.staged_script,
        &blueprint.remote_script,
        false,
    ) {
        Ok(output) => append_command_detail(&mut detail_parts, "Uploaded remote script", &output),
        Err(err) => {
            return CommandOutcome {
                status: BootstrapStepStatus::Failed,
                duration: start.elapsed(),
                detail: Some(err),
            };
        }
    }

    if let Some(staged_payload) = blueprint.staged_payload.as_ref() {
        match run_scp_path(&blueprint.ssh, staged_payload, &blueprint.remote_dir, true) {
            Ok(output) => {
                append_command_detail(&mut detail_parts, "Uploaded payload directory", &output)
            }
            Err(err) => {
                return CommandOutcome {
                    status: BootstrapStepStatus::Failed,
                    duration: start.elapsed(),
                    detail: Some(err),
                };
            }
        }
    } else {
        detail_parts.push("No payload directory configured; only script uploaded.".to_string());
    }

    match run_ssh_shell(
        &blueprint.ssh,
        format!("chmod +x {}", shell_quote(&blueprint.remote_script)),
    ) {
        Ok(output) => append_command_detail(&mut detail_parts, "Marked script executable", &output),
        Err(err) => {
            return CommandOutcome {
                status: BootstrapStepStatus::Failed,
                duration: start.elapsed(),
                detail: Some(err),
            };
        }
    }
    detail_parts.push(match blueprint_remote_payload_summary(blueprint) {
        Some(bytes) => format!("Uploaded assets to {} ({}).", blueprint.remote_dir, bytes),
        None => format!("Uploaded script to {}.", blueprint.remote_dir),
    });

    CommandOutcome {
        status: BootstrapStepStatus::Success,
        duration: start.elapsed(),
        detail: Some(detail_parts.join(" ")),
    }
}

fn blueprint_remote_payload_summary(blueprint: &BootstrapBlueprint) -> Option<String> {
    blueprint
        .staged_payload
        .as_ref()
        .map(|_| format!("{} bytes", blueprint.payload_bytes))
}

fn execute_remote(blueprint: &BootstrapBlueprint) -> ApplyOutcome {
    let start = Instant::now();
    let run_id = generate_run_id();
    let apply_script = build_apply_command(blueprint, &run_id);

    println!("APPLY SCRIPT: {}", apply_script);

    match run_ssh_shell(&blueprint.ssh, apply_script) {
        Ok(output) => {
            let mut completion = ApplyCompletion::Success;
            let mut detail_parts = vec![format!(
                "Guest bootstrap script completed via `{}`.",
                output.command
            )];

            for line in output.stdout.lines() {
                let trimmed = line.trim();
                if let Some(reason) = trimmed.strip_prefix(SENTINEL_ERROR_PREFIX) {
                    return ApplyOutcome {
                        command: CommandOutcome {
                            status: BootstrapStepStatus::Failed,
                            duration: start.elapsed(),
                            detail: Some(format!(
                                "Bootstrap script reported error: {}",
                                reason.trim()
                            )),
                        },
                        completion: ApplyCompletion::Success,
                    };
                }
                if trimmed == SENTINEL_NOOP {
                    completion = ApplyCompletion::NoOp;
                    detail_parts.push("Bootstrap script reported no-op.".to_string());
                }
            }

            if let Some(snippet) = summarize_output("stdout", &output.stdout) {
                detail_parts.push(snippet);
            }
            if let Some(snippet) = summarize_output("stderr", &output.stderr) {
                detail_parts.push(snippet);
            }

            ApplyOutcome {
                command: CommandOutcome {
                    status: BootstrapStepStatus::Success,
                    duration: start.elapsed(),
                    detail: Some(detail_parts.join(" ")),
                },
                completion,
            }
        }
        Err(err) => ApplyOutcome {
            command: CommandOutcome {
                status: BootstrapStepStatus::Failed,
                duration: start.elapsed(),
                detail: Some(err),
            },
            completion: ApplyCompletion::Success,
        },
    }
}

fn verify_remote(blueprint: &BootstrapBlueprint) -> CommandOutcome {
    let start = Instant::now();
    let mut detail_parts = Vec::new();

    if let Some(command) = blueprint.verify.command.as_ref() {
        let verify_script = build_verify_command(blueprint, command);
        match run_ssh_shell(&blueprint.ssh, verify_script) {
            Ok(output) => {
                append_command_detail(&mut detail_parts, "Verification command succeeded", &output)
            }
            Err(err) => {
                return CommandOutcome {
                    status: BootstrapStepStatus::Failed,
                    duration: start.elapsed(),
                    detail: Some(format!("Verification command failed: {err}")),
                };
            }
        }
    }

    if let Some(path) = blueprint.verify.path.as_ref() {
        let resolved_path = if blueprint.verify.path_is_relative {
            format!("{}/{}", blueprint.remote_dir, path)
        } else {
            path.clone()
        };
        let script = format!("test -e {}", shell_quote(&resolved_path));
        match run_ssh_shell(&blueprint.ssh, script) {
            Ok(output) => append_command_detail(
                &mut detail_parts,
                "Verification path check succeeded",
                &output,
            ),
            Err(err) => {
                return CommandOutcome {
                    status: BootstrapStepStatus::Failed,
                    duration: start.elapsed(),
                    detail: Some(format!(
                        "Verification path check failed for {}: {err}",
                        resolved_path
                    )),
                };
            }
        }
        detail_parts.push(format!("Verified remote path {} exists.", resolved_path));
    }

    let detail = if detail_parts.is_empty() {
        Some("No verification checks configured.".to_string())
    } else {
        Some(detail_parts.join(" "))
    };

    CommandOutcome {
        status: BootstrapStepStatus::Success,
        duration: start.elapsed(),
        detail,
    }
}

fn prepare_remote_directory(
    blueprint: &BootstrapBlueprint,
) -> std::result::Result<ProcessOutput, String> {
    let script = format!(
        "rm -rf {} && mkdir -p {}",
        shell_quote(&blueprint.remote_dir),
        shell_quote(&blueprint.remote_dir)
    );
    run_ssh_shell(&blueprint.ssh, script)
}

fn build_apply_command(blueprint: &BootstrapBlueprint, run_id: &str) -> String {
    let mut script = String::new();
    script.push_str("set -euo pipefail;");
    script.push_str(&format!("mkdir -p {};", shell_quote(&blueprint.remote_dir)));
    script.push_str(&format!("cd {};", shell_quote(&blueprint.remote_dir)));
    script.push_str(&format!("export CASTRA_VM={};", shell_quote(&blueprint.vm)));
    script.push_str(&format!("export CASTRA_RUN_ID={};", shell_quote(run_id)));
    let payload_dir = blueprint
        .remote_payload_dir
        .as_deref()
        .unwrap_or(&blueprint.remote_dir);
    script.push_str(&format!(
        "export CASTRA_PAYLOAD_DIR={};",
        shell_quote(payload_dir)
    ));
    let mut env_entries: Vec<_> = blueprint.env.iter().collect();
    env_entries.sort_by(|a, b| a.0.cmp(b.0));
    for (key, value) in env_entries {
        script.push_str(&format!("export {}={};", key, shell_quote(value)));
    }
    script.push_str(&format!("./{}", STAGED_SCRIPT_NAME));
    script
}

fn build_verify_command(blueprint: &BootstrapBlueprint, command: &str) -> String {
    let mut script = String::new();
    script.push_str("set -euo pipefail;");
    script.push_str(&format!("cd {};", shell_quote(&blueprint.remote_dir)));
    script.push_str(&format!("export CASTRA_VM={};", shell_quote(&blueprint.vm)));
    let payload_dir = blueprint
        .remote_payload_dir
        .as_deref()
        .unwrap_or(&blueprint.remote_dir);
    script.push_str(&format!(
        "export CASTRA_PAYLOAD_DIR={};",
        shell_quote(payload_dir)
    ));
    let mut env_entries: Vec<_> = blueprint.env.iter().collect();
    env_entries.sort_by(|a, b| a.0.cmp(b.0));
    for (key, value) in env_entries {
        script.push_str(&format!("export {}={};", key, shell_quote(value)));
    }
    script.push_str(command);
    script
}

fn run_ssh_shell(ssh: &SshConfig, script: String) -> std::result::Result<ProcessOutput, String> {
    run_ssh_command_capture(
        ssh,
        &[
            String::from("sh"),
            String::from("-lc"),
            format!("\"{}\"", script),
        ],
    )
}

fn run_ssh_command_capture(
    ssh: &SshConfig,
    remote_args: &[String],
) -> std::result::Result<ProcessOutput, String> {
    let mut args = Vec::new();
    if let Some(identity) = ssh.identity.as_ref() {
        args.push(String::from("-i"));
        args.push(identity.display().to_string());
    }
    for option in &ssh.options {
        args.push(String::from("-o"));
        args.push(option.clone());
    }
    args.push(String::from("-p"));
    args.push(ssh.port.to_string());
    println!("ARGS: {:?}", args);
    args.push(format!("{}@{}", ssh.user, ssh.host));
    println!("ARGS: {:?}", args);
    args.extend(remote_args.iter().cloned());

    run_command("ssh", &args)
}

fn run_scp_path(
    ssh: &SshConfig,
    local: &Path,
    remote_destination: &str,
    recursive: bool,
) -> std::result::Result<ProcessOutput, String> {
    let mut args = Vec::new();
    if let Some(identity) = ssh.identity.as_ref() {
        args.push(String::from("-i"));
        args.push(identity.display().to_string());
    }
    for option in &ssh.options {
        args.push(String::from("-o"));
        args.push(option.clone());
    }
    args.push(String::from("-P"));
    args.push(ssh.port.to_string());
    if recursive {
        args.push(String::from("-r"));
    }
    args.push(local.display().to_string());
    args.push(format!("{}@{}:{}", ssh.user, ssh.host, remote_destination,));

    run_command("scp", &args)
}

fn escape_scp_destination(path: &str) -> String {
    shell_quote(path)
}

fn run_command(program: &str, args: &[String]) -> std::result::Result<ProcessOutput, String> {
    let command_repr = format_cli(program, args);
    let mut command = Command::new(program);
    command.args(args);
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());

    let output = command.output().map_err(|err| match err.kind() {
        io::ErrorKind::NotFound => {
            format!("Command `{program}` not found in PATH while executing bootstrap step.")
        }
        _ => format!("Failed to execute `{program}`: {err}"),
    })?;

    if output.status.success() {
        Ok(ProcessOutput {
            command: command_repr,
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        })
    } else {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!(
            "`{}` exited with code {:?}. stdout: {} stderr: {}",
            command_repr,
            output.status.code(),
            stdout.trim(),
            stderr.trim()
        ))
    }
}

fn shell_quote(input: &str) -> String {
    let mut result = String::from("'");
    for ch in input.chars() {
        if ch == '\'' {
            result.push_str("'\\''");
        } else {
            result.push(ch);
        }
    }
    result.push('\'');
    result
}

fn elapsed_ms(duration: Duration) -> u64 {
    duration.as_millis().min(u128::from(u64::MAX)) as u64
}

#[derive(Serialize)]
struct BootstrapRunLog {
    vm: String,
    artifact_hash: String,
    base_hash: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    stamp: Option<String>,
    status: String,
    duration_ms: u64,
    steps: Vec<StepRecord>,
    script_source: String,
    staged_script: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    payload_source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    staged_payload: Option<String>,
    payload_bytes: u64,
    remote_dir: String,
    ssh_user: String,
    ssh_host: String,
    ssh_port: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    ssh_identity: Option<String>,
    env: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    metadata_path: Option<String>,
}

#[derive(Serialize)]
struct StepRecord {
    step: String,
    status: String,
    duration_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    detail: Option<String>,
}

impl BootstrapRunLog {
    fn success(
        vm: &str,
        blueprint: &BootstrapBlueprint,
        base_hash: &str,
        steps: Vec<StepLog>,
        duration_ms: u64,
        status: BootstrapStatus,
    ) -> Self {
        let status_str = match status {
            BootstrapStatus::Success => "success",
            BootstrapStatus::NoOp => "noop",
        };
        Self {
            vm: vm.to_string(),
            artifact_hash: blueprint.artifact_hash.clone(),
            base_hash: base_hash.to_string(),
            stamp: None,
            status: status_str.to_string(),
            duration_ms,
            steps: steps.into_iter().map(StepRecord::from).collect(),
            script_source: blueprint.script_source.display().to_string(),
            staged_script: blueprint.staged_script.display().to_string(),
            payload_source: blueprint
                .payload_source
                .as_ref()
                .map(|path| path.display().to_string()),
            staged_payload: blueprint
                .staged_payload
                .as_ref()
                .map(|path| path.display().to_string()),
            payload_bytes: blueprint.payload_bytes,
            remote_dir: blueprint.remote_dir.clone(),
            ssh_user: blueprint.ssh.user.clone(),
            ssh_host: blueprint.ssh.host.clone(),
            ssh_port: blueprint.ssh.port,
            ssh_identity: blueprint
                .ssh
                .identity
                .as_ref()
                .map(|path| path.display().to_string()),
            env: blueprint
                .env
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            metadata_path: blueprint
                .metadata_path
                .as_ref()
                .map(|path| path.display().to_string()),
        }
    }

    fn failure(
        vm: &str,
        blueprint: &BootstrapBlueprint,
        base_hash: &str,
        steps: Vec<StepLog>,
        duration_ms: u64,
        error: String,
    ) -> Self {
        let mut records: Vec<StepRecord> = steps.into_iter().map(StepRecord::from).collect();
        records.push(StepRecord {
            step: "error".to_string(),
            status: "failed".to_string(),
            duration_ms: 0,
            detail: Some(error),
        });

        Self {
            vm: vm.to_string(),
            artifact_hash: blueprint.artifact_hash.clone(),
            base_hash: base_hash.to_string(),
            stamp: None,
            status: "failed".to_string(),
            duration_ms,
            steps: records,
            script_source: blueprint.script_source.display().to_string(),
            staged_script: blueprint.staged_script.display().to_string(),
            payload_source: blueprint
                .payload_source
                .as_ref()
                .map(|path| path.display().to_string()),
            staged_payload: blueprint
                .staged_payload
                .as_ref()
                .map(|path| path.display().to_string()),
            payload_bytes: blueprint.payload_bytes,
            remote_dir: blueprint.remote_dir.clone(),
            ssh_user: blueprint.ssh.user.clone(),
            ssh_host: blueprint.ssh.host.clone(),
            ssh_port: blueprint.ssh.port,
            ssh_identity: blueprint
                .ssh
                .identity
                .as_ref()
                .map(|path| path.display().to_string()),
            env: blueprint
                .env
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            metadata_path: blueprint
                .metadata_path
                .as_ref()
                .map(|path| path.display().to_string()),
        }
    }
}

impl From<StepLog> for StepRecord {
    fn from(log: StepLog) -> Self {
        Self {
            step: format_step(log.kind),
            status: format_step_status(log.status),
            duration_ms: log.duration_ms,
            detail: log.detail,
        }
    }
}

fn format_step(kind: BootstrapStepKind) -> String {
    match kind {
        BootstrapStepKind::WaitHandshake => "wait-handshake",
        BootstrapStepKind::Connect => "connect",
        BootstrapStepKind::Transfer => "transfer",
        BootstrapStepKind::Apply => "apply",
        BootstrapStepKind::Verify => "verify",
    }
    .to_string()
}

fn format_step_status(status: BootstrapStepStatus) -> String {
    match status {
        BootstrapStepStatus::Success => "success",
        BootstrapStepStatus::Skipped => "skipped",
        BootstrapStepStatus::Failed => "failed",
    }
    .to_string()
}

fn write_run_log(dir: &Path, log: &BootstrapRunLog) -> io::Result<PathBuf> {
    fs::create_dir_all(dir)?;
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_secs(0))
        .as_secs();
    let filename = format!("{}-{}.json", log.vm, timestamp);
    let path = dir.join(filename);
    let payload = serde_json::to_vec_pretty(log).map_err(|err| {
        io::Error::new(
            io::ErrorKind::Other,
            format!("Failed to serialize bootstrap log: {err}"),
        )
    })?;
    fs::write(&path, payload)?;
    Ok(path)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::config::BaseImageSource;
    use crate::config::{
        BootstrapConfig, BootstrapMode, BrokerConfig, DEFAULT_BROKER_PORT, LifecycleConfig,
        MemorySpec, PortForward, PortProtocol, ProjectConfig, VmBootstrapConfig, VmDefinition,
        Workflows,
    };
    use crate::core::diagnostics::{Diagnostic, Severity};
    use crate::core::events::{BootstrapPlanAction, BootstrapStatus, BootstrapTrigger, Event};
    use crate::core::outcome::BootstrapRunStatus;
    use crate::core::runtime::{AssetPreparation, ResolvedVmAssets, RuntimeContext};
    use serde_json::json;
    use std::collections::HashMap;
    use std::env;
    use std::ffi::OsString;
    use std::fs::{self, File};
    use std::io::{self, Write};
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};
    use std::sync::{Mutex, OnceLock};
    use std::time::{SystemTime, UNIX_EPOCH};
    use tempfile::TempDir;

    #[derive(Default)]
    struct RecordingReporter {
        events: Vec<Event>,
    }

    impl Reporter for RecordingReporter {
        fn report(&mut self, event: Event) {
            self.events.push(event);
        }
    }

    impl RecordingReporter {
        fn take(self) -> Vec<Event> {
            self.events
        }
    }

    static PATH_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    struct PathGuard {
        original: Option<OsString>,
    }

    impl PathGuard {
        fn prepend(dir: &Path) -> Self {
            let original = env::var_os("PATH");
            let mut paths = match &original {
                Some(value) => env::split_paths(value).collect::<Vec<_>>(),
                None => Vec::new(),
            };
            paths.insert(0, dir.to_path_buf());
            let joined = env::join_paths(paths).expect("failed to join PATH entries");
            unsafe { env::set_var("PATH", &joined) };
            Self { original }
        }
    }

    impl Drop for PathGuard {
        fn drop(&mut self) {
            if let Some(original) = self.original.take() {
                unsafe { env::set_var("PATH", original) };
            } else {
                unsafe { env::remove_var("PATH") };
            }
        }
    }

    fn write_executable(dir: &Path, name: &str, body: &str) -> io::Result<PathBuf> {
        let path = dir.join(name);
        let mut file = File::create(&path)?;
        file.write_all(body.as_bytes())?;
        drop(file);
        let mut perms = fs::metadata(&path)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&path, perms)?;
        Ok(path)
    }

    #[test]
    fn bootstrap_pipeline_skips_when_skip_mode()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let _env_guard = PATH_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let temp_dir = TempDir::new()?;
        let workspace = temp_dir.path();

        let state_root = workspace.join("state");
        fs::create_dir_all(state_root.join("logs"))?;
        fs::create_dir_all(state_root.join("images"))?;

        let base_image_path = workspace.join("base.img");
        fs::write(&base_image_path, b"base-image")?;

        let context = RuntimeContext {
            state_root: state_root.clone(),
            log_root: state_root.join("logs"),
            qemu_system: PathBuf::from("/usr/bin/false"),
            qemu_img: None,
            accelerators: Vec::new(),
        };

        let vm = VmDefinition {
            name: "devbox".to_string(),
            role_name: "devbox".to_string(),
            replica_index: 0,
            description: None,
            base_image: BaseImageSource::from_explicit(base_image_path),
            overlay: state_root.join("overlays/devbox.qcow2"),
            cpus: 2,
            memory: MemorySpec::new("2048 MiB", Some(2 * 1024 * 1024 * 1024)),
            port_forwards: Vec::new(),
            bootstrap: VmBootstrapConfig {
                mode: BootstrapMode::Skip,
                script: Some(PathBuf::from("/tmp/bootstrap-script")),
                payload: Some(PathBuf::from("/tmp/bootstrap-payload")),
                handshake_timeout_secs: 30,
                remote_dir: PathBuf::from(DEFAULT_REMOTE_BASE),
                env: HashMap::new(),
                verify: None,
            },
        };

        let project = ProjectConfig {
            file_path: workspace.join("castra.toml"),
            project_root: workspace.to_path_buf(),
            version: "0.2.0".to_string(),
            project_name: "demo".to_string(),
            vms: vec![vm],
            state_root: state_root.clone(),
            workflows: Workflows { init: Vec::new() },
            broker: BrokerConfig {
                port: DEFAULT_BROKER_PORT,
            },
            lifecycle: LifecycleConfig::default(),
            bootstrap: BootstrapConfig::default(),
            warnings: Vec::new(),
        };

        let preparations = vec![AssetPreparation {
            assets: ResolvedVmAssets { boot: None },
            overlay_created: false,
            overlay_reclaimed_bytes: None,
            events: Vec::new(),
        }];

        let mut reporter = RecordingReporter::default();
        let mut diagnostics: Vec<Diagnostic> = Vec::new();
        let outcomes = run_all(
            &project,
            &context,
            &preparations,
            &mut reporter,
            &mut diagnostics,
        )?;

        assert_eq!(outcomes.len(), 1);
        let outcome = &outcomes[0];
        assert_eq!(outcome.vm, "devbox");
        assert_eq!(outcome.status, BootstrapRunStatus::Skipped);
        assert!(outcome.stamp.is_none());
        assert!(outcome.log_path.is_none());

        let events = reporter.take();
        assert!(
            events.is_empty(),
            "bootstrap should emit no events when bootstrap mode is skip"
        );

        assert!(
            diagnostics
                .iter()
                .any(|diag| diag.severity == Severity::Info)
        );

        Ok(())
    }

    #[test]
    fn bootstrap_plan_reports_run_details() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let _env_guard = PATH_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let temp_dir = TempDir::new()?;
        let workspace = temp_dir.path();

        let project_root = workspace.join("project");
        let script_source = project_root.join("bootstrap").join("devbox").join("run.sh");
        fs::create_dir_all(script_source.parent().unwrap())?;
        fs::write(&script_source, b"#!/bin/sh\nexit 0\n")?;
        let payload_source = script_source.parent().unwrap().join("payload");
        fs::create_dir_all(&payload_source)?;
        fs::write(payload_source.join("data.txt"), b"payload")?;

        let base_image_path = workspace.join("base.img");
        fs::write(&base_image_path, b"base-image")?;

        let vm = VmDefinition {
            name: "devbox".to_string(),
            role_name: "devbox".to_string(),
            replica_index: 0,
            description: None,
            base_image: BaseImageSource::from_explicit(base_image_path),
            overlay: workspace.join("state/overlays/devbox.qcow2"),
            cpus: 2,
            memory: MemorySpec::new("2048 MiB", Some(2 * 1024 * 1024 * 1024)),
            port_forwards: vec![PortForward {
                host: 2222,
                guest: 22,
                protocol: PortProtocol::Tcp,
            }],
            bootstrap: VmBootstrapConfig {
                mode: BootstrapMode::Auto,
                script: Some(script_source.clone()),
                payload: Some(payload_source.clone()),
                handshake_timeout_secs: 45,
                remote_dir: PathBuf::from(DEFAULT_REMOTE_BASE),
                env: HashMap::new(),
                verify: None,
            },
        };

        let project = ProjectConfig {
            file_path: project_root.join("castra.toml"),
            project_root: project_root.clone(),
            version: "0.2.0".to_string(),
            project_name: "demo".to_string(),
            vms: vec![vm],
            state_root: workspace.join("state"),
            workflows: Workflows { init: Vec::new() },
            broker: BrokerConfig {
                port: DEFAULT_BROKER_PORT,
            },
            lifecycle: LifecycleConfig::default(),
            bootstrap: BootstrapConfig::default(),
            warnings: Vec::new(),
        };

        let mut diagnostics = Vec::new();
        let mut reporter = RecordingReporter::default();
        let plans = plan_all(&project, &mut reporter, &mut diagnostics)?;

        assert!(
            diagnostics.is_empty(),
            "unexpected diagnostics: {diagnostics:?}"
        );
        assert_eq!(plans.len(), 1);
        let plan = &plans[0];
        assert_eq!(plan.action, BootstrapPlanAction::WouldRun);
        assert_eq!(plan.trigger, Some(BootstrapTrigger::Auto));
        assert_eq!(plan.handshake_timeout_secs, Some(45));
        assert_eq!(plan.payload_bytes, Some("payload".len() as u64));
        assert_eq!(
            plan.remote_dir.as_deref(),
            Some("/tmp/castra-bootstrap/devbox")
        );
        assert!(plan.env_keys.is_empty());
        assert!(plan.warnings.is_empty());

        let events = reporter.take();
        assert!(matches!(
            events.as_slice(),
            [Event::BootstrapPlanned { .. }]
        ));

        Ok(())
    }

    #[test]
    fn bootstrap_plan_marks_missing_script() -> std::result::Result<(), Box<dyn std::error::Error>>
    {
        let _env_guard = PATH_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let temp_dir = TempDir::new()?;
        let workspace = temp_dir.path();

        let project_root = workspace.join("project");
        fs::create_dir_all(project_root.join("bootstrap"))?;

        let base_image_path = workspace.join("base.img");
        fs::write(&base_image_path, b"base-image")?;

        let vm = VmDefinition {
            name: "devbox".to_string(),
            role_name: "devbox".to_string(),
            replica_index: 0,
            description: None,
            base_image: BaseImageSource::from_explicit(base_image_path),
            overlay: workspace.join("state/overlays/devbox.qcow2"),
            cpus: 2,
            memory: MemorySpec::new("2048 MiB", Some(2 * 1024 * 1024 * 1024)),
            port_forwards: Vec::new(),
            bootstrap: VmBootstrapConfig {
                mode: BootstrapMode::Auto,
                script: None,
                payload: None,
                handshake_timeout_secs: 30,
                remote_dir: PathBuf::from(DEFAULT_REMOTE_BASE),
                env: HashMap::new(),
                verify: None,
            },
        };

        let project = ProjectConfig {
            file_path: project_root.join("castra.toml"),
            project_root: project_root.clone(),
            version: "0.2.0".to_string(),
            project_name: "demo".to_string(),
            vms: vec![vm],
            state_root: workspace.join("state"),
            workflows: Workflows { init: Vec::new() },
            broker: BrokerConfig {
                port: DEFAULT_BROKER_PORT,
            },
            lifecycle: LifecycleConfig::default(),
            bootstrap: BootstrapConfig::default(),
            warnings: Vec::new(),
        };

        let mut diagnostics = Vec::new();
        let mut reporter = RecordingReporter::default();
        let plans = plan_all(&project, &mut reporter, &mut diagnostics)?;

        assert!(diagnostics.is_empty());
        assert_eq!(plans.len(), 1);
        let plan = &plans[0];
        assert_eq!(plan.action, BootstrapPlanAction::WouldSkip);
        assert!(plan.reason.contains("not configured"));

        let events = reporter.take();
        assert!(matches!(
            events.as_slice(),
            [Event::BootstrapPlanned { .. }]
        ));

        Ok(())
    }

    #[test]
    fn bootstrap_pipeline_runs_successfully() -> std::result::Result<(), Box<dyn std::error::Error>>
    {
        run_success_scenario("success")
    }

    #[test]
    fn bootstrap_pipeline_emits_noop_on_sentinel()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        run_success_scenario("noop")
    }

    fn run_success_scenario(mode: &str) -> std::result::Result<(), Box<dyn std::error::Error>> {
        let _env_guard = PATH_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let temp_dir = TempDir::new()?;
        let workspace = temp_dir.path();

        let state_root = workspace.join("state");
        fs::create_dir_all(state_root.join("handshakes"))?;
        fs::create_dir_all(state_root.join("logs"))?;
        fs::create_dir_all(state_root.join("images"))?;

        let handshake_path = state_root.join("handshakes").join("devbox.json");
        let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
        let handshake_payload = json!({ "timestamp": now });
        fs::write(&handshake_path, serde_json::to_vec(&handshake_payload)?)?;

        let project_root = workspace.join("project");
        let script_source = project_root.join("bootstrap").join("devbox").join("run.sh");
        fs::create_dir_all(script_source.parent().unwrap())?;
        fs::write(&script_source, b"#!/bin/sh\necho running bootstrap\n")?;
        let payload_source = script_source.parent().unwrap().join("payload");
        fs::create_dir_all(&payload_source)?;
        fs::write(payload_source.join("data.txt"), b"payload data")?;

        let base_image_path = workspace.join("base.img");
        fs::write(&base_image_path, b"base-image")?;

        let bin_dir = workspace.join("bin");
        fs::create_dir_all(&bin_dir)?;
        write_executable(
            &bin_dir,
            "ssh",
            "#!/bin/sh\nlog=\"$MOCK_SSH_LOG\"\necho \"$@\" >> \"$log\"\nmode=\"$MOCK_SSH_MODE\"\nif printf '%s' \"$@\" | grep -q './run.sh'; then\n  if [ \"$mode\" = noop ]; then\n    echo 'Castra:noop'\n  elif [ \"$mode\" = error ]; then\n    echo 'Castra:error:script aborted'\n  fi\nfi\nif [ \"$mode\" = fail_connect ] && printf '%s' \"$@\" | grep -q ' true'; then\n  exit 1\nfi\nexit 0\n",
        )?;
        write_executable(
            &bin_dir,
            "scp",
            "#!/bin/sh\nlog=\"$MOCK_SCP_LOG\"\necho \"$@\" >> \"$log\"\nexit 0\n",
        )?;
        write_executable(&bin_dir, "qemu-system-x86_64", "#!/bin/sh\nexit 0\n")?;
        let _path_guard = PathGuard::prepend(&bin_dir);

        unsafe {
            env::set_var("MOCK_SSH_LOG", workspace.join("ssh.log"));
            env::set_var("MOCK_SCP_LOG", workspace.join("scp.log"));
            env::set_var("MOCK_SSH_MODE", mode);
        }

        let context = RuntimeContext {
            state_root: state_root.clone(),
            log_root: state_root.join("logs"),
            qemu_system: bin_dir.join("qemu-system-x86_64"),
            qemu_img: None,
            accelerators: Vec::new(),
        };

        let vm = VmDefinition {
            name: "devbox".to_string(),
            role_name: "devbox".to_string(),
            replica_index: 0,
            description: None,
            base_image: BaseImageSource::from_explicit(base_image_path),
            overlay: state_root.join("overlays/devbox.qcow2"),
            cpus: 2,
            memory: MemorySpec::new("2048 MiB", Some(2 * 1024 * 1024 * 1024)),
            port_forwards: vec![PortForward {
                host: 2222,
                guest: 22,
                protocol: PortProtocol::Tcp,
            }],
            bootstrap: VmBootstrapConfig {
                mode: BootstrapMode::Auto,
                script: Some(script_source.clone()),
                payload: Some(payload_source.clone()),
                handshake_timeout_secs: 30,
                remote_dir: PathBuf::from(DEFAULT_REMOTE_BASE),
                env: HashMap::new(),
                verify: None,
            },
        };

        let project = ProjectConfig {
            file_path: project_root.join("castra.toml"),
            project_root: project_root.clone(),
            version: "0.2.0".to_string(),
            project_name: "demo".to_string(),
            vms: vec![vm],
            state_root: state_root.clone(),
            workflows: Workflows { init: Vec::new() },
            broker: BrokerConfig {
                port: DEFAULT_BROKER_PORT,
            },
            lifecycle: LifecycleConfig::default(),
            bootstrap: BootstrapConfig::default(),
            warnings: Vec::new(),
        };

        let preparations = vec![AssetPreparation {
            assets: ResolvedVmAssets { boot: None },
            overlay_created: false,
            overlay_reclaimed_bytes: None,
            events: Vec::new(),
        }];

        let mut reporter = RecordingReporter::default();
        let mut diagnostics: Vec<Diagnostic> = Vec::new();
        let outcomes = run_all(
            &project,
            &context,
            &preparations,
            &mut reporter,
            &mut diagnostics,
        )?;

        assert_eq!(outcomes.len(), 1);
        let outcome = &outcomes[0];
        assert_eq!(outcome.vm, "devbox");
        if mode == "noop" {
            assert_eq!(outcome.status, BootstrapRunStatus::NoOp);
        } else {
            assert_eq!(outcome.status, BootstrapRunStatus::Success);
        }
        assert!(outcome.log_path.is_some());

        let events = reporter.take();
        let completed = events.iter().find_map(|event| match event {
            Event::BootstrapCompleted { status, .. } => Some(*status),
            _ => None,
        });
        assert!(completed.is_some());
        if mode == "noop" {
            assert_eq!(completed.unwrap(), BootstrapStatus::NoOp);
        } else {
            assert_eq!(completed.unwrap(), BootstrapStatus::Success);
        }

        assert!(diagnostics.is_empty());
        Ok(())
    }
}
