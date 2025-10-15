use std::fs;
use std::io::{self, Read};
use std::panic;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::config::{BaseImageSource, BootstrapMode, ProjectConfig, VmDefinition};
use crate::core::diagnostics::{Diagnostic, Severity};
use crate::core::events::{
    BootstrapStatus, BootstrapStepKind, BootstrapStepStatus, BootstrapTrigger, Event,
};
use crate::core::outcome::{BootstrapRunOutcome, BootstrapRunStatus};
use crate::core::reporter::Reporter;
use crate::core::runtime::{AssetPreparation, RuntimeContext};
use crate::core::status::HANDSHAKE_FRESHNESS;
use crate::error::{Error, Result};
use crate::managed::ManagedArtifactKind;
use sha2::{Digest, Sha256};

const PLAN_FILE_NAME: &str = "plan.json";
const STAMP_DIR_NAME: &str = "stamps";
const LOG_SUBDIR: &str = "bootstrap";

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

    let (event_tx, event_rx) = mpsc::channel::<Event>();
    let mut first_error: Option<Error> = None;
    let mut vm_slots: Vec<Option<BootstrapRunOutcome>> =
        (0..project.vms.len()).map(|_| None).collect();

    let state_root = context.state_root.clone();
    let log_root = context.log_root.clone();

    std::thread::scope(|scope| {
        let mut handles = Vec::new();

        for (index, (vm, prep)) in project.vms.iter().zip(preparations.iter()).enumerate() {
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
                    prep,
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

fn run_for_vm(
    state_root: &Path,
    log_root: &Path,
    vm: &VmDefinition,
    prep: &AssetPreparation,
    emit_event: &mut dyn FnMut(Event),
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<BootstrapRunOutcome> {
    match vm.bootstrap.mode {
        BootstrapMode::Disabled => {
            diagnostics.push(Diagnostic::new(
                Severity::Info,
                format!("Bootstrap disabled for VM `{}`; skipping.", vm.name),
            ));
            return Ok(BootstrapRunOutcome {
                vm: vm.name.clone(),
                status: BootstrapRunStatus::Skipped,
                stamp: None,
                log_path: None,
            });
        }
        BootstrapMode::Auto | BootstrapMode::Always => {}
    }

    let plan_dir = state_root.join("bootstrap").join(&vm.name);
    let plan_path = plan_dir.join(PLAN_FILE_NAME);
    if !plan_path.is_file() {
        if matches!(vm.bootstrap.mode, BootstrapMode::Always) {
            return Err(Error::BootstrapFailed {
                vm: vm.name.clone(),
                message: format!(
                    "Bootstrap plan not found at {} (required by mode=always).",
                    plan_path.display()
                ),
            });
        }

        diagnostics.push(
            Diagnostic::new(
                Severity::Info,
                format!(
                    "No bootstrap plan for VM `{}` (expected {}). Skipping.",
                    vm.name,
                    plan_path.display()
                ),
            )
            .with_help("Run workflows.init to stage bootstrap artifacts under the state root."),
        );

        return Ok(BootstrapRunOutcome {
            vm: vm.name.clone(),
            status: BootstrapRunStatus::Skipped,
            stamp: None,
            log_path: None,
        });
    }

    let plan = load_plan(&plan_path)?;
    let base_hash = derive_base_hash(vm, prep)?;
    let trigger = if matches!(vm.bootstrap.mode, BootstrapMode::Always) {
        BootstrapTrigger::Always
    } else {
        BootstrapTrigger::Auto
    };

    emit_event(Event::BootstrapStarted {
        vm: vm.name.clone(),
        base_hash: base_hash.clone(),
        artifact_hash: plan.artifact_hash.clone(),
        trigger,
    });

    let start = Instant::now();
    let mut steps = Vec::new();
    let log_dir = log_root.join(LOG_SUBDIR);
    let stamps_dir = plan_dir.join(STAMP_DIR_NAME);
    fs::create_dir_all(&stamps_dir).map_err(|err| Error::BootstrapFailed {
        vm: vm.name.clone(),
        message: format!(
            "Failed to prepare stamp directory {}: {err}",
            stamps_dir.display()
        ),
    })?;

    let stamp_id = build_stamp_id(&base_hash, &plan.artifact_hash);
    let stamp_path = stamps_dir.join(format!("{stamp_id}.json"));

    if matches!(vm.bootstrap.mode, BootstrapMode::Auto) && stamp_path.is_file() {
        let duration_ms = elapsed_ms(start.elapsed());
        steps.push(StepLog::skipped(
            BootstrapStepKind::WaitHandshake,
            "Existing stamp matches artifact; no work required.",
        ));
        emit_event(Event::BootstrapStep {
            vm: vm.name.clone(),
            step: BootstrapStepKind::WaitHandshake,
            status: BootstrapStepStatus::Skipped,
            duration_ms: 0,
            detail: Some("Bootstrap stamp already satisfied.".to_string()),
        });
        emit_event(Event::BootstrapCompleted {
            vm: vm.name.clone(),
            status: BootstrapStatus::NoOp,
            duration_ms,
            stamp: Some(stamp_id.clone()),
        });

        let log_path = write_run_log(
            &log_dir,
            &BootstrapRunLog::noop(&vm.name, &plan, &base_hash, &stamp_id, steps, duration_ms),
        )
        .map_err(|err| Error::BootstrapFailed {
            vm: vm.name.clone(),
            message: format!("Failed to persist bootstrap log: {err}"),
        })?;

        return Ok(BootstrapRunOutcome {
            vm: vm.name.clone(),
            status: BootstrapRunStatus::NoOp,
            stamp: Some(stamp_id),
            log_path: Some(log_path),
        });
    }

    let handshake_start = Instant::now();
    let handshake_result = wait_for_handshake(state_root, &vm.name, plan.handshake_timeout);
    let handshake_duration = handshake_start.elapsed();

    match handshake_result {
        Ok(handshake_ts) => {
            steps.push(StepLog::success(
                BootstrapStepKind::WaitHandshake,
                handshake_duration,
                Some(format!("Fresh handshake observed at {:?}.", handshake_ts)),
            ));
            emit_event(Event::BootstrapStep {
                vm: vm.name.clone(),
                step: BootstrapStepKind::WaitHandshake,
                status: BootstrapStepStatus::Success,
                duration_ms: elapsed_ms(handshake_duration),
                detail: Some("Handshake fresh".to_string()),
            });
        }
        Err(err) => {
            let failure_detail = match &err {
                Error::BootstrapFailed { message, .. } => message.clone(),
                other => other.to_string(),
            };
            steps.push(StepLog::from_result(
                BootstrapStepKind::WaitHandshake,
                BootstrapStepStatus::Failed,
                handshake_duration,
                Some(failure_detail.clone()),
            ));
            emit_event(Event::BootstrapStep {
                vm: vm.name.clone(),
                step: BootstrapStepKind::WaitHandshake,
                status: BootstrapStepStatus::Failed,
                duration_ms: elapsed_ms(handshake_duration),
                detail: Some(failure_detail.clone()),
            });
            let duration_ms = elapsed_ms(start.elapsed());
            emit_event(Event::BootstrapFailed {
                vm: vm.name.clone(),
                duration_ms,
                error: failure_detail.clone(),
            });
            write_run_log(
                &log_dir,
                &BootstrapRunLog::failure(
                    &vm.name,
                    &plan,
                    &base_hash,
                    None,
                    steps,
                    duration_ms,
                    failure_detail.clone(),
                ),
            )
            .map_err(|err| Error::BootstrapFailed {
                vm: vm.name.clone(),
                message: format!("Failed to persist bootstrap log: {err}"),
            })?;
            return Err(err);
        }
    }

    let connect_res = check_connectivity(&plan);
    let connect_duration = connect_res.duration;
    emit_event(Event::BootstrapStep {
        vm: vm.name.clone(),
        step: BootstrapStepKind::Connect,
        status: connect_res.status,
        duration_ms: elapsed_ms(connect_duration),
        detail: connect_res.detail.clone(),
    });
    steps.push(StepLog::from_result(
        BootstrapStepKind::Connect,
        connect_res.status,
        connect_duration,
        connect_res.detail.clone(),
    ));
    if !matches!(connect_res.status, BootstrapStepStatus::Success) {
        let duration_ms = elapsed_ms(start.elapsed());
        let failure_detail = connect_res
            .detail
            .clone()
            .unwrap_or_else(|| "Failed to establish SSH connectivity.".to_string());
        emit_event(Event::BootstrapFailed {
            vm: vm.name.clone(),
            duration_ms,
            error: failure_detail.clone(),
        });
        write_run_log(
            &log_dir,
            &BootstrapRunLog::failure(
                &vm.name,
                &plan,
                &base_hash,
                None,
                steps,
                duration_ms,
                failure_detail.clone(),
            ),
        )
        .map_err(|err| Error::BootstrapFailed {
            vm: vm.name.clone(),
            message: format!("Failed to persist bootstrap log: {err}"),
        })?;

        return Err(Error::BootstrapFailed {
            vm: vm.name.clone(),
            message: failure_detail,
        });
    }

    let transfer_res = transfer_artifacts(&plan);
    let transfer_duration = transfer_res.duration;
    emit_event(Event::BootstrapStep {
        vm: vm.name.clone(),
        step: BootstrapStepKind::Transfer,
        status: transfer_res.status,
        duration_ms: elapsed_ms(transfer_duration),
        detail: transfer_res.detail.clone(),
    });
    steps.push(StepLog::from_result(
        BootstrapStepKind::Transfer,
        transfer_res.status,
        transfer_duration,
        transfer_res.detail.clone(),
    ));
    if !matches!(transfer_res.status, BootstrapStepStatus::Success) {
        let duration_ms = elapsed_ms(start.elapsed());
        let failure_detail = transfer_res
            .detail
            .clone()
            .unwrap_or_else(|| "Failed to transfer artifacts.".to_string());
        emit_event(Event::BootstrapFailed {
            vm: vm.name.clone(),
            duration_ms,
            error: failure_detail.clone(),
        });
        write_run_log(
            &log_dir,
            &BootstrapRunLog::failure(
                &vm.name,
                &plan,
                &base_hash,
                None,
                steps,
                duration_ms,
                failure_detail.clone(),
            ),
        )
        .map_err(|err| Error::BootstrapFailed {
            vm: vm.name.clone(),
            message: format!("Failed to persist bootstrap log: {err}"),
        })?;

        return Err(Error::BootstrapFailed {
            vm: vm.name.clone(),
            message: failure_detail,
        });
    }

    let apply_res = execute_remote(&plan);
    let apply_duration = apply_res.duration;
    emit_event(Event::BootstrapStep {
        vm: vm.name.clone(),
        step: BootstrapStepKind::Apply,
        status: apply_res.status,
        duration_ms: elapsed_ms(apply_duration),
        detail: apply_res.detail.clone(),
    });
    steps.push(StepLog::from_result(
        BootstrapStepKind::Apply,
        apply_res.status,
        apply_duration,
        apply_res.detail.clone(),
    ));
    if !matches!(apply_res.status, BootstrapStepStatus::Success) {
        let duration_ms = elapsed_ms(start.elapsed());
        let failure_detail = apply_res
            .detail
            .clone()
            .unwrap_or_else(|| "Remote bootstrap execution failed.".to_string());
        emit_event(Event::BootstrapFailed {
            vm: vm.name.clone(),
            duration_ms,
            error: failure_detail.clone(),
        });
        write_run_log(
            &log_dir,
            &BootstrapRunLog::failure(
                &vm.name,
                &plan,
                &base_hash,
                None,
                steps,
                duration_ms,
                failure_detail.clone(),
            ),
        )
        .map_err(|err| Error::BootstrapFailed {
            vm: vm.name.clone(),
            message: format!("Failed to persist bootstrap log: {err}"),
        })?;

        return Err(Error::BootstrapFailed {
            vm: vm.name.clone(),
            message: failure_detail,
        });
    }

    let verify_res = verify_outcome(&plan, &stamp_path, &stamp_id, &base_hash, &plan_path);
    let verify_duration = verify_res.duration;
    emit_event(Event::BootstrapStep {
        vm: vm.name.clone(),
        step: BootstrapStepKind::Verify,
        status: verify_res.status,
        duration_ms: elapsed_ms(verify_duration),
        detail: verify_res.detail.clone(),
    });
    steps.push(StepLog::from_result(
        BootstrapStepKind::Verify,
        verify_res.status,
        verify_duration,
        verify_res.detail.clone(),
    ));
    if !matches!(verify_res.status, BootstrapStepStatus::Success) {
        let duration_ms = elapsed_ms(start.elapsed());
        let failure_detail = verify_res
            .detail
            .clone()
            .unwrap_or_else(|| "Bootstrap verification failed.".to_string());
        emit_event(Event::BootstrapFailed {
            vm: vm.name.clone(),
            duration_ms,
            error: failure_detail.clone(),
        });
        write_run_log(
            &log_dir,
            &BootstrapRunLog::failure(
                &vm.name,
                &plan,
                &base_hash,
                Some(stamp_id.clone()),
                steps,
                duration_ms,
                failure_detail.clone(),
            ),
        )
        .map_err(|err| Error::BootstrapFailed {
            vm: vm.name.clone(),
            message: format!("Failed to persist bootstrap log: {err}"),
        })?;

        return Err(Error::BootstrapFailed {
            vm: vm.name.clone(),
            message: failure_detail,
        });
    }

    let total_ms = elapsed_ms(start.elapsed());
    emit_event(Event::BootstrapCompleted {
        vm: vm.name.clone(),
        status: BootstrapStatus::Success,
        duration_ms: total_ms,
        stamp: Some(stamp_id.clone()),
    });

    let log_record =
        BootstrapRunLog::success(&vm.name, &plan, &base_hash, &stamp_id, steps, total_ms);
    let log_path = write_run_log(&log_dir, &log_record).map_err(|err| Error::BootstrapFailed {
        vm: vm.name.clone(),
        message: format!("Failed to persist bootstrap log: {err}"),
    })?;

    Ok(BootstrapRunOutcome {
        vm: vm.name.clone(),
        status: BootstrapRunStatus::Success,
        stamp: Some(stamp_id),
        log_path: Some(log_path),
    })
}

fn wait_for_handshake(state_root: &Path, vm: &str, timeout: Duration) -> Result<SystemTime> {
    let handshake_path = state_root.join("handshakes").join(format!("{vm}.json"));
    let deadline = Instant::now() + timeout;

    loop {
        if let Some(timestamp) = read_handshake_timestamp(&handshake_path)? {
            let now = SystemTime::now();
            if now
                .duration_since(timestamp)
                .unwrap_or_else(|_| Duration::from_secs(0))
                <= HANDSHAKE_FRESHNESS
            {
                return Ok(timestamp);
            }
        }

        if Instant::now() >= deadline {
            return Err(Error::BootstrapFailed {
                vm: vm.to_string(),
                message: format!(
                    "Timed out waiting for fresh broker handshake after {} seconds.",
                    timeout.as_secs()
                ),
            });
        }

        std::thread::sleep(Duration::from_secs(2));
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

fn derive_base_hash(vm: &VmDefinition, prep: &AssetPreparation) -> Result<String> {
    match &vm.base_image {
        BaseImageSource::Managed(_) => {
            if let Some(managed) = &prep.managed {
                if let Some(summary) = managed
                    .verification
                    .artifacts
                    .iter()
                    .find(|artifact| artifact.kind == ManagedArtifactKind::RootDisk)
                {
                    return Ok(summary.final_sha256.clone());
                }

                return compute_file_sha256(&managed.paths.root_disk).map_err(|err| {
                    Error::BootstrapFailed {
                        vm: vm.name.clone(),
                        message: err,
                    }
                });
            }

            Err(Error::BootstrapFailed {
                vm: vm.name.clone(),
                message: "Managed base image verification missing for bootstrap.".to_string(),
            })
        }
        BaseImageSource::Path(path) => {
            compute_file_sha256(path).map_err(|err| Error::BootstrapFailed {
                vm: vm.name.clone(),
                message: err,
            })
        }
    }
}

fn compute_file_sha256(path: &Path) -> std::result::Result<String, String> {
    let mut file = fs::File::open(path).map_err(|err| {
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

fn build_stamp_id(base_hash: &str, artifact_hash: &str) -> String {
    format!(
        "{}__{}",
        sanitize_for_filename(base_hash),
        sanitize_for_filename(artifact_hash)
    )
}

fn sanitize_for_filename(value: &str) -> String {
    value
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect()
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

    fn skipped(kind: BootstrapStepKind, message: &str) -> Self {
        Self {
            kind,
            status: BootstrapStepStatus::Skipped,
            duration_ms: 0,
            detail: Some(message.to_string()),
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

fn check_connectivity(plan: &BootstrapPlan) -> CommandOutcome {
    let start = Instant::now();
    match run_ssh_command(plan, "true") {
        Ok(_) => CommandOutcome {
            status: BootstrapStepStatus::Success,
            duration: start.elapsed(),
            detail: Some("SSH connectivity confirmed.".to_string()),
        },
        Err(err) => CommandOutcome {
            status: BootstrapStepStatus::Failed,
            duration: start.elapsed(),
            detail: Some(err),
        },
    }
}

fn transfer_artifacts(plan: &BootstrapPlan) -> CommandOutcome {
    if plan.uploads.is_empty() {
        return CommandOutcome {
            status: BootstrapStepStatus::Skipped,
            duration: Duration::from_millis(0),
            detail: Some("No uploads configured.".to_string()),
        };
    }

    let start = Instant::now();
    for upload in &plan.uploads {
        if let Err(err) = run_scp(plan, upload) {
            return CommandOutcome {
                status: BootstrapStepStatus::Failed,
                duration: start.elapsed(),
                detail: Some(err),
            };
        }
    }

    CommandOutcome {
        status: BootstrapStepStatus::Success,
        duration: start.elapsed(),
        detail: Some(format!("Transferred {} artifact(s).", plan.uploads.len())),
    }
}

fn execute_remote(plan: &BootstrapPlan) -> CommandOutcome {
    let start = Instant::now();
    let command = plan.render_remote_command();
    match run_ssh_command(plan, &command) {
        Ok(_) => CommandOutcome {
            status: BootstrapStepStatus::Success,
            duration: start.elapsed(),
            detail: Some("Guest bootstrap script completed.".to_string()),
        },
        Err(err) => CommandOutcome {
            status: BootstrapStepStatus::Failed,
            duration: start.elapsed(),
            detail: Some(err),
        },
    }
}

fn verify_outcome(
    plan: &BootstrapPlan,
    stamp_path: &Path,
    stamp_id: &str,
    base_hash: &str,
    plan_path: &Path,
) -> CommandOutcome {
    let start = Instant::now();

    if let Some(remote) = plan.remote_verify_path.as_deref() {
        if let Err(err) = run_ssh_command(plan, &format!("test -e {}", remote)) {
            return CommandOutcome {
                status: BootstrapStepStatus::Failed,
                duration: start.elapsed(),
                detail: Some(format!("Remote verification failed: {err}")),
            };
        }
    }

    if let Err(err) = write_stamp(
        stamp_path,
        stamp_id,
        base_hash,
        &plan.artifact_hash,
        plan_path,
    ) {
        return CommandOutcome {
            status: BootstrapStepStatus::Failed,
            duration: start.elapsed(),
            detail: Some(err),
        };
    }

    CommandOutcome {
        status: BootstrapStepStatus::Success,
        duration: start.elapsed(),
        detail: Some("Bootstrap stamp recorded.".to_string()),
    }
}

fn write_stamp(
    path: &Path,
    stamp_id: &str,
    base_hash: &str,
    artifact_hash: &str,
    plan_path: &Path,
) -> std::result::Result<(), String> {
    #[derive(Serialize)]
    struct Stamp<'a> {
        stamp: &'a str,
        base_hash: &'a str,
        artifact_hash: &'a str,
        plan: String,
        recorded_at: u64,
    }

    let record = Stamp {
        stamp: stamp_id,
        base_hash,
        artifact_hash,
        plan: plan_path.display().to_string(),
        recorded_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_else(|_| Duration::from_secs(0))
            .as_secs(),
    };

    let payload = serde_json::to_vec_pretty(&record)
        .map_err(|err| format!("Failed to encode bootstrap stamp: {err}"))?;
    fs::write(path, payload)
        .map_err(|err| format!("Failed to write bootstrap stamp {}: {err}", path.display()))
}

fn elapsed_ms(duration: Duration) -> u64 {
    duration.as_millis().min(u128::from(u64::MAX)) as u64
}

fn run_ssh_command(plan: &BootstrapPlan, command: &str) -> std::result::Result<(), String> {
    let mut args = Vec::new();
    if let Some(identity) = plan.ssh_identity.as_ref() {
        args.push("-i".to_string());
        args.push(identity.display().to_string());
    }
    for option in &plan.ssh_options {
        args.push("-o".to_string());
        args.push(option.clone());
    }
    args.push("-p".to_string());
    args.push(plan.ssh_port.to_string());
    args.push(format!("{}@{}", plan.ssh_user, plan.ssh_host));
    args.push(command.to_string());

    run_command("ssh", &args)
}

fn run_scp(plan: &BootstrapPlan, upload: &UploadSpec) -> std::result::Result<(), String> {
    let mut args = Vec::new();
    if let Some(identity) = plan.ssh_identity.as_ref() {
        args.push("-i".to_string());
        args.push(identity.display().to_string());
    }
    for option in &plan.ssh_options {
        args.push("-o".to_string());
        args.push(option.clone());
    }
    args.push("-P".to_string());
    args.push(plan.ssh_port.to_string());
    if upload.recursive {
        args.push("-r".to_string());
    }
    args.push(upload.source.display().to_string());
    args.push(format!(
        "{}@{}:{}",
        plan.ssh_user, plan.ssh_host, upload.destination
    ));

    run_command("scp", &args)
}

fn run_command(program: &str, args: &[String]) -> std::result::Result<(), String> {
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
        Ok(())
    } else {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!(
            "`{program}` exited with code {:?}. stdout: {} stderr: {}",
            output.status.code(),
            stdout.trim(),
            stderr.trim()
        ))
    }
}

fn load_plan(path: &Path) -> Result<BootstrapPlan> {
    let contents = fs::read(path).map_err(|err| Error::BootstrapFailed {
        vm: path
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|s| s.to_str())
            .unwrap_or("<unknown>")
            .to_string(),
        message: format!("Failed to read bootstrap plan {}: {err}", path.display()),
    })?;

    let stored: StoredPlan =
        serde_json::from_slice(&contents).map_err(|err| Error::BootstrapFailed {
            vm: path
                .parent()
                .and_then(|p| p.file_name())
                .and_then(|s| s.to_str())
                .unwrap_or("<unknown>")
                .to_string(),
            message: format!("Failed to parse bootstrap plan {}: {err}", path.display()),
        })?;

    stored.into_plan(path)
}

#[derive(Deserialize)]
struct StoredPlan {
    artifact_hash: String,
    #[serde(default)]
    handshake_timeout_secs: Option<u64>,
    ssh: StoredPlanSsh,
    remote: StoredPlanRemote,
    #[serde(default)]
    uploads: Vec<StoredPlanUpload>,
}

#[derive(Deserialize)]
struct StoredPlanSsh {
    user: String,
    #[serde(default = "StoredPlanSsh::default_host")]
    host: String,
    #[serde(default = "StoredPlanSsh::default_port")]
    port: u16,
    #[serde(default)]
    identity: Option<PathBuf>,
    #[serde(default)]
    options: Vec<String>,
}

impl StoredPlanSsh {
    fn default_host() -> String {
        "127.0.0.1".to_string()
    }

    fn default_port() -> u16 {
        22
    }
}

#[derive(Deserialize)]
struct StoredPlanRemote {
    bootstrap_script: String,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    verify_path: Option<String>,
}

#[derive(Deserialize)]
struct StoredPlanUpload {
    source: PathBuf,
    destination: String,
    #[serde(default)]
    recursive: Option<bool>,
}

struct BootstrapPlan {
    artifact_hash: String,
    handshake_timeout: Duration,
    ssh_user: String,
    ssh_host: String,
    ssh_port: u16,
    ssh_identity: Option<PathBuf>,
    ssh_options: Vec<String>,
    uploads: Vec<UploadSpec>,
    remote_script: String,
    remote_args: Vec<String>,
    remote_verify_path: Option<String>,
}

impl BootstrapPlan {
    fn render_remote_command(&self) -> String {
        if self.remote_args.is_empty() {
            self.remote_script.clone()
        } else {
            let mut command = String::with_capacity(64);
            command.push_str(&self.remote_script);
            for arg in &self.remote_args {
                command.push(' ');
                command.push_str(arg);
            }
            command
        }
    }
}

impl StoredPlan {
    fn into_plan(self, path: &Path) -> Result<BootstrapPlan> {
        let base_dir = path.parent().unwrap_or_else(|| Path::new("."));

        let handshake_timeout = self
            .handshake_timeout_secs
            .map(Duration::from_secs)
            .unwrap_or_else(|| {
                Duration::from_secs(crate::config::DEFAULT_BOOTSTRAP_HANDSHAKE_WAIT_SECS)
            });

        if self.ssh.user.trim().is_empty() {
            return Err(Error::BootstrapFailed {
                vm: base_dir
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("<unknown>")
                    .to_string(),
                message: "Bootstrap plan missing ssh.user.".to_string(),
            });
        }

        let uploads = self
            .uploads
            .into_iter()
            .map(|upload| upload.resolve(base_dir))
            .collect::<Result<Vec<_>>>()?;

        Ok(BootstrapPlan {
            artifact_hash: self.artifact_hash,
            handshake_timeout,
            ssh_user: self.ssh.user,
            ssh_host: self.ssh.host,
            ssh_port: self.ssh.port,
            ssh_identity: self.ssh.identity,
            ssh_options: self.ssh.options,
            uploads,
            remote_script: self.remote.bootstrap_script,
            remote_args: self.remote.args,
            remote_verify_path: self.remote.verify_path,
        })
    }
}

impl StoredPlanUpload {
    fn resolve(self, base_dir: &Path) -> Result<UploadSpec> {
        let source = if self.source.is_absolute() {
            self.source
        } else {
            base_dir.join(self.source)
        };

        if !source.exists() {
            return Err(Error::BootstrapFailed {
                vm: base_dir
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("<unknown>")
                    .to_string(),
                message: format!(
                    "Bootstrap upload source {} does not exist.",
                    source.display()
                ),
            });
        }

        let recursive = self.recursive.unwrap_or_else(|| source.is_dir());

        Ok(UploadSpec {
            source,
            destination: self.destination,
            recursive,
        })
    }
}

struct UploadSpec {
    source: PathBuf,
    destination: String,
    recursive: bool,
}

#[derive(Serialize)]
struct BootstrapRunLog {
    vm: String,
    artifact_hash: String,
    base_hash: String,
    stamp: Option<String>,
    status: String,
    duration_ms: u64,
    steps: Vec<StepRecord>,
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
        plan: &BootstrapPlan,
        base_hash: &str,
        stamp_id: &str,
        steps: Vec<StepLog>,
        duration_ms: u64,
    ) -> Self {
        Self {
            vm: vm.to_string(),
            artifact_hash: plan.artifact_hash.clone(),
            base_hash: base_hash.to_string(),
            stamp: Some(stamp_id.to_string()),
            status: "success".to_string(),
            duration_ms,
            steps: steps.into_iter().map(StepRecord::from).collect(),
        }
    }

    fn failure(
        vm: &str,
        plan: &BootstrapPlan,
        base_hash: &str,
        stamp_id: Option<String>,
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
            artifact_hash: plan.artifact_hash.clone(),
            base_hash: base_hash.to_string(),
            stamp: stamp_id,
            status: "failed".to_string(),
            duration_ms,
            steps: records,
        }
    }

    fn noop(
        vm: &str,
        plan: &BootstrapPlan,
        base_hash: &str,
        stamp_id: &str,
        steps: Vec<StepLog>,
        duration_ms: u64,
    ) -> Self {
        Self {
            vm: vm.to_string(),
            artifact_hash: plan.artifact_hash.clone(),
            base_hash: base_hash.to_string(),
            stamp: Some(stamp_id.to_string()),
            status: "noop".to_string(),
            duration_ms,
            steps: steps.into_iter().map(StepRecord::from).collect(),
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
    use crate::config::{
        BaseImageSource, BootstrapConfig, BootstrapMode, BrokerConfig, DEFAULT_BROKER_PORT,
        MemorySpec, ProjectConfig, VmBootstrapConfig, VmDefinition, Workflows,
    };
    use crate::core::events::{
        BootstrapStatus, BootstrapStepKind, BootstrapStepStatus, BootstrapTrigger, Event,
    };
    use crate::core::outcome::BootstrapRunStatus;
    use crate::core::reporter::Reporter;
    use crate::core::runtime::{AssetPreparation, ResolvedVmAssets, RuntimeContext};
    use crate::error::Error;
    use crate::{ImageManager, LifecycleConfig};
    use serde_json::json;
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

    fn normalize_sha_result(result: std::result::Result<String, String>) -> io::Result<String> {
        result.map_err(|err| io::Error::new(io::ErrorKind::Other, err))
    }

    #[test]
    fn bootstrap_pipeline_runs_and_is_idempotent()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let _env_guard = PATH_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let temp_dir = TempDir::new()?;
        let workspace = temp_dir.path();

        let state_root = workspace.join("state");
        fs::create_dir_all(state_root.join("handshakes"))?;
        fs::create_dir_all(state_root.join("bootstrap").join("devbox"))?;
        fs::create_dir_all(state_root.join("logs"))?;
        fs::create_dir_all(state_root.join("images"))?;

        let handshake_path = state_root.join("handshakes").join("devbox.json");
        let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
        let handshake_payload = json!({ "timestamp": now });
        fs::write(&handshake_path, serde_json::to_vec(&handshake_payload)?)?;

        let plan_dir = state_root.join("bootstrap").join("devbox");
        let plan_path = plan_dir.join("plan.json");
        let upload_payload_path = plan_dir.join("payload.txt");
        fs::write(&upload_payload_path, b"payload")?;
        let plan_payload = json!({
            "artifact_hash": "artifact-hash-v1",
            "handshake_timeout_secs": 15,
            "ssh": {
                "user": "root",
                "host": "127.0.0.1",
                "port": 22,
                "options": ["StrictHostKeyChecking=no"]
            },
            "remote": {
                "bootstrap_script": "/bin/true",
                "args": []
            },
            "uploads": [
                {
                    "source": "payload.txt",
                    "destination": "/tmp/payload.txt"
                }
            ]
        });
        fs::write(&plan_path, serde_json::to_vec_pretty(&plan_payload)?)?;

        let base_image_path = workspace.join("base.img");
        fs::write(&base_image_path, b"test-image-contents")?;

        let bin_dir = workspace.join("bin");
        fs::create_dir_all(&bin_dir)?;
        write_executable(&bin_dir, "ssh", "#!/bin/sh\nexit 0\n")?;
        write_executable(&bin_dir, "scp", "#!/bin/sh\nexit 0\n")?;
        write_executable(&bin_dir, "qemu-system-x86_64", "#!/bin/sh\nexit 0\n")?;
        let _path_guard = PathGuard::prepend(&bin_dir);

        let image_manager =
            ImageManager::new(state_root.join("images"), state_root.join("logs"), None);
        let context = RuntimeContext {
            state_root: state_root.clone(),
            log_root: state_root.join("logs"),
            qemu_system: bin_dir.join("qemu-system-x86_64"),
            qemu_img: None,
            image_manager,
            accelerators: Vec::new(),
        };

        let vm = VmDefinition {
            name: "devbox".to_string(),
            role_name: "devbox".to_string(),
            replica_index: 0,
            description: None,
            base_image: BaseImageSource::Path(base_image_path.clone()),
            overlay: state_root.join("overlays").join("devbox.qcow2"),
            cpus: 2,
            memory: MemorySpec::new("2048 MiB", Some(2 * 1024 * 1024 * 1024)),
            port_forwards: Vec::new(),
            bootstrap: VmBootstrapConfig {
                mode: BootstrapMode::Auto,
            },
        };

        let project = ProjectConfig {
            file_path: workspace.join("castra.toml"),
            version: "0.2.0".to_string(),
            project_name: "demo".to_string(),
            vms: vec![vm],
            state_root: state_root.clone(),
            workflows: Workflows { init: Vec::new() },
            broker: BrokerConfig {
                port: DEFAULT_BROKER_PORT,
            },
            lifecycle: LifecycleConfig::default(),
            bootstrap: BootstrapConfig {
                mode: BootstrapMode::Auto,
            },
            warnings: Vec::new(),
        };

        let preparations = vec![AssetPreparation {
            assets: ResolvedVmAssets { boot: None },
            managed: None,
            overlay_created: false,
        }];

        let mut reporter = RecordingReporter::default();
        let mut diagnostics = Vec::new();
        let outcomes = run_all(
            &project,
            &context,
            &preparations,
            &mut reporter,
            &mut diagnostics,
        )?;

        assert!(
            diagnostics.is_empty(),
            "unexpected diagnostics: {diagnostics:?}"
        );
        assert_eq!(outcomes.len(), 1);
        let outcome = &outcomes[0];
        assert_eq!(outcome.vm, "devbox");
        assert_eq!(outcome.status, BootstrapRunStatus::Success);

        let base_hash = normalize_sha_result(compute_file_sha256(&base_image_path))?;
        let expected_stamp = build_stamp_id(&base_hash, "artifact-hash-v1");
        assert_eq!(outcome.stamp.as_deref(), Some(expected_stamp.as_str()));

        let log_path = outcome
            .log_path
            .as_ref()
            .expect("success outcome should provide a log path");
        assert!(
            log_path.is_file(),
            "bootstrap log should be written at {}",
            log_path.display()
        );
        let log_json: serde_json::Value = serde_json::from_slice(&fs::read(log_path)?)?;
        assert_eq!(log_json["status"], "success");
        assert_eq!(log_json["vm"], "devbox");

        let stamp_path = plan_dir
            .join("stamps")
            .join(format!("{expected_stamp}.json"));
        assert!(
            stamp_path.is_file(),
            "stamp should be recorded at {}",
            stamp_path.display()
        );

        let events = reporter.take();
        assert!(
            events.len() >= 7,
            "expected lifecycle events for bootstrap run, found {}",
            events.len()
        );
        let mut iter = events.iter();

        match iter.next().expect("bootstrap started event") {
            Event::BootstrapStarted {
                vm,
                base_hash: event_base,
                artifact_hash,
                trigger,
                ..
            } => {
                assert_eq!(vm, "devbox");
                assert_eq!(event_base, &base_hash);
                assert_eq!(artifact_hash, "artifact-hash-v1");
                assert_eq!(*trigger, BootstrapTrigger::Auto);
            }
            other => panic!("unexpected first event: {other:?}"),
        }

        let expected_steps = [
            (
                BootstrapStepKind::WaitHandshake,
                BootstrapStepStatus::Success,
            ),
            (BootstrapStepKind::Connect, BootstrapStepStatus::Success),
            (BootstrapStepKind::Transfer, BootstrapStepStatus::Success),
            (BootstrapStepKind::Apply, BootstrapStepStatus::Success),
            (BootstrapStepKind::Verify, BootstrapStepStatus::Success),
        ];

        for (expected_kind, expected_status) in expected_steps {
            match iter.next().expect("bootstrap step event") {
                Event::BootstrapStep {
                    vm, step, status, ..
                } => {
                    assert_eq!(vm, "devbox");
                    assert_eq!(*step, expected_kind);
                    assert_eq!(*status, expected_status);
                }
                other => panic!("unexpected event in step sequence: {other:?}"),
            }
        }

        match iter.next().expect("bootstrap completion event") {
            Event::BootstrapCompleted {
                vm, status, stamp, ..
            } => {
                assert_eq!(vm, "devbox");
                assert_eq!(*status, BootstrapStatus::Success);
                assert_eq!(stamp.as_deref(), Some(expected_stamp.as_str()));
            }
            other => panic!("unexpected completion event: {other:?}"),
        }

        assert!(
            iter.next().is_none(),
            "no additional events expected after completion"
        );

        let mut reporter_noop = RecordingReporter::default();
        let mut diagnostics_noop = Vec::new();
        let outcomes_noop = run_all(
            &project,
            &context,
            &preparations,
            &mut reporter_noop,
            &mut diagnostics_noop,
        )?;

        assert!(
            diagnostics_noop.is_empty(),
            "unexpected diagnostics during noop run: {diagnostics_noop:?}"
        );
        assert_eq!(outcomes_noop.len(), 1);
        let noop_outcome = &outcomes_noop[0];
        assert_eq!(noop_outcome.vm, "devbox");
        assert_eq!(noop_outcome.status, BootstrapRunStatus::NoOp);
        assert_eq!(noop_outcome.stamp.as_deref(), Some(expected_stamp.as_str()));

        let noop_log_path = noop_outcome
            .log_path
            .as_ref()
            .expect("noop outcome should still log run metadata");
        assert!(
            noop_log_path.is_file(),
            "noop bootstrap log should exist at {}",
            noop_log_path.display()
        );
        let noop_log: serde_json::Value = serde_json::from_slice(&fs::read(noop_log_path)?)?;
        assert_eq!(noop_log["status"], "noop");
        assert_eq!(noop_log["vm"], "devbox");

        let noop_events = reporter_noop.take();
        let mut iter = noop_events.iter();

        match iter.next().expect("noop bootstrap started event") {
            Event::BootstrapStarted {
                vm,
                base_hash: event_base,
                artifact_hash,
                trigger,
                ..
            } => {
                assert_eq!(vm, "devbox");
                assert_eq!(event_base, &base_hash);
                assert_eq!(artifact_hash, "artifact-hash-v1");
                assert_eq!(*trigger, BootstrapTrigger::Auto);
            }
            other => panic!("unexpected first noop event: {other:?}"),
        }

        match iter.next().expect("noop wait handshake step") {
            Event::BootstrapStep {
                vm, step, status, ..
            } => {
                assert_eq!(vm, "devbox");
                assert_eq!(*step, BootstrapStepKind::WaitHandshake);
                assert_eq!(*status, BootstrapStepStatus::Skipped);
            }
            other => panic!("unexpected noop step event: {other:?}"),
        }

        match iter.next().expect("noop completion event") {
            Event::BootstrapCompleted {
                vm, status, stamp, ..
            } => {
                assert_eq!(vm, "devbox");
                assert_eq!(*status, BootstrapStatus::NoOp);
                assert_eq!(stamp.as_deref(), Some(expected_stamp.as_str()));
            }
            other => panic!("unexpected noop completion event: {other:?}"),
        }

        assert!(
            iter.next().is_none(),
            "no additional events expected during noop run"
        );

        Ok(())
    }

    #[test]
    fn bootstrap_pipeline_reports_handshake_failure()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let _env_guard = PATH_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let temp_dir = TempDir::new()?;
        let workspace = temp_dir.path();

        let state_root = workspace.join("state");
        fs::create_dir_all(state_root.join("handshakes"))?;
        fs::create_dir_all(state_root.join("bootstrap").join("devbox"))?;
        fs::create_dir_all(state_root.join("logs"))?;
        fs::create_dir_all(state_root.join("images"))?;
        fs::create_dir_all(state_root.join("overlays"))?;

        let plan_dir = state_root.join("bootstrap").join("devbox");
        let plan_path = plan_dir.join("plan.json");
        let plan_payload = json!({
            "artifact_hash": "artifact-hash-v1",
            "handshake_timeout_secs": 0,
            "ssh": {
                "user": "root",
                "host": "127.0.0.1",
                "port": 22,
                "options": []
            },
            "remote": {
                "bootstrap_script": "/bin/true",
                "args": []
            }
        });
        fs::write(&plan_path, serde_json::to_vec_pretty(&plan_payload)?)?;

        let base_image_path = workspace.join("base.img");
        fs::write(&base_image_path, b"test-image-contents")?;

        let bin_dir = workspace.join("bin");
        fs::create_dir_all(&bin_dir)?;
        write_executable(&bin_dir, "ssh", "#!/bin/sh\nexit 0\n")?;
        write_executable(&bin_dir, "scp", "#!/bin/sh\nexit 0\n")?;
        write_executable(&bin_dir, "qemu-system-x86_64", "#!/bin/sh\nexit 0\n")?;
        let _path_guard = PathGuard::prepend(&bin_dir);

        let image_manager =
            ImageManager::new(state_root.join("images"), state_root.join("logs"), None);
        let context = RuntimeContext {
            state_root: state_root.clone(),
            log_root: state_root.join("logs"),
            qemu_system: bin_dir.join("qemu-system-x86_64"),
            qemu_img: None,
            image_manager,
            accelerators: Vec::new(),
        };

        let vm = VmDefinition {
            name: "devbox".to_string(),
            role_name: "devbox".to_string(),
            replica_index: 0,
            description: None,
            base_image: BaseImageSource::Path(base_image_path.clone()),
            overlay: state_root.join("overlays").join("devbox.qcow2"),
            cpus: 2,
            memory: MemorySpec::new("2048 MiB", Some(2 * 1024 * 1024 * 1024)),
            port_forwards: Vec::new(),
            bootstrap: VmBootstrapConfig {
                mode: BootstrapMode::Auto,
            },
        };

        let project = ProjectConfig {
            file_path: workspace.join("castra.toml"),
            version: "0.2.0".to_string(),
            project_name: "demo".to_string(),
            vms: vec![vm],
            state_root: state_root.clone(),
            workflows: Workflows { init: Vec::new() },
            broker: BrokerConfig {
                port: DEFAULT_BROKER_PORT,
            },
            lifecycle: LifecycleConfig::default(),
            bootstrap: BootstrapConfig {
                mode: BootstrapMode::Auto,
            },
            warnings: Vec::new(),
        };

        let preparations = vec![AssetPreparation {
            assets: ResolvedVmAssets { boot: None },
            managed: None,
            overlay_created: false,
        }];

        let mut reporter = RecordingReporter::default();
        let mut diagnostics = Vec::new();
        let err = run_all(
            &project,
            &context,
            &preparations,
            &mut reporter,
            &mut diagnostics,
        )
        .expect_err("handshake timeout should abort bootstrap");

        assert!(
            diagnostics.is_empty(),
            "diagnostics should remain empty when handshake fails"
        );

        match err {
            Error::BootstrapFailed { vm, message } => {
                assert_eq!(vm, "devbox");
                assert_eq!(
                    message,
                    "Timed out waiting for fresh broker handshake after 0 seconds."
                );
            }
            other => panic!("unexpected error: {other:?}"),
        }

        let events = reporter.take();
        assert_eq!(
            events.len(),
            3,
            "expected started, wait-handshake failure, and failure events"
        );
        match &events[0] {
            Event::BootstrapStarted { vm, .. } => assert_eq!(vm, "devbox"),
            other => panic!("unexpected first event: {other:?}"),
        }
        match &events[1] {
            Event::BootstrapStep {
                vm,
                step,
                status,
                detail,
                ..
            } => {
                assert_eq!(vm, "devbox");
                assert_eq!(*step, BootstrapStepKind::WaitHandshake);
                assert_eq!(*status, BootstrapStepStatus::Failed);
                assert_eq!(
                    detail.as_deref(),
                    Some("Timed out waiting for fresh broker handshake after 0 seconds.")
                );
            }
            other => panic!("unexpected second event: {other:?}"),
        }
        match &events[2] {
            Event::BootstrapFailed { vm, error, .. } => {
                assert_eq!(vm, "devbox");
                assert_eq!(
                    error,
                    "Timed out waiting for fresh broker handshake after 0 seconds."
                );
            }
            other => panic!("unexpected third event: {other:?}"),
        }

        let log_dir = context.log_root.join("bootstrap");
        let mut log_paths = Vec::new();
        for entry in fs::read_dir(&log_dir)? {
            log_paths.push(entry?.path());
        }
        assert_eq!(log_paths.len(), 1, "expected single bootstrap log file");
        let log_json: serde_json::Value = serde_json::from_slice(&fs::read(&log_paths[0])?)?;
        assert_eq!(log_json["status"], "failed");
        assert_eq!(log_json["vm"], "devbox");
        assert_eq!(log_json["steps"][0]["step"], "wait-handshake");
        assert_eq!(log_json["steps"][0]["status"], "failed");
        assert_eq!(
            log_json["steps"][0]["detail"],
            "Timed out waiting for fresh broker handshake after 0 seconds."
        );
        assert_eq!(log_json["steps"][1]["step"], "error");

        Ok(())
    }
}
