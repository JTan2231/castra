use std::panic;
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;
use std::time::{Instant, SystemTime};

mod bus;
mod clean;

use crate::config::{self, ProjectConfig, VmDefinition};
use crate::error::{Error, Result};
use crate::managed::{
    ManagedArtifactKind, ManagedImageArtifactExpectation, ManagedImageArtifactSummary,
    ManagedImagePaths, ManagedImageProfileOutcome, ManagedImageSpec,
    ManagedImageVerificationOutcome,
};

use super::bootstrap;
use super::broker as broker_core;
use super::diagnostics::{Diagnostic, Severity};
use super::events::{
    EphemeralCleanupReason, Event, ManagedImageArtifactPlan, ManagedImageArtifactReport,
    ManagedImageChecksum, ManagedImageSpecHandle, ShutdownOutcome,
};
use super::logs as logs_core;
use super::options::{
    BootstrapOverrides, BrokerOptions, BusPublishOptions, BusTailOptions, CleanOptions,
    ConfigLoadOptions, DownOptions, InitOptions, LogsOptions, PortsOptions, StatusOptions,
    UpOptions,
};
use super::outcome::{
    BootstrapRunStatus, BrokerLaunchOutcome, BrokerShutdownOutcome, BusPublishOutcome,
    BusTailOutcome, CleanOutcome, DownOutcome, InitOutcome, LogsOutcome, ManagedVmAssets,
    OperationOutput, OperationResult, PortsOutcome, StatusOutcome, UpOutcome, VmLaunchOutcome,
    VmShutdownOutcome,
};
use super::ports as ports_core;
use super::project::{
    ProjectLoad, config_state_root, default_config_contents, default_project_name, load_project,
    preferred_init_target,
};
use super::reporter::Reporter;
use super::runtime::{
    BootOverrides, BrokerProcessState, CheckOutcome, ManagedAcquisition, RuntimeContext,
    ShutdownTimeouts, check_disk_space, check_host_capacity, ensure_ports_available,
    ensure_vm_assets, launch_vm, prepare_runtime_context, shutdown_broker, shutdown_vm,
    start_broker,
};
use super::status as status_core;

pub fn init(
    mut options: InitOptions,
    reporter: Option<&mut dyn Reporter>,
) -> OperationResult<InitOutcome> {
    let target_path = preferred_init_target(&options);
    let project_name = options
        .project_name
        .take()
        .unwrap_or_else(|| default_project_name(&target_path));
    let state_root = config::default_state_root(&project_name, &target_path);

    let target_exists = target_path.exists();
    if target_exists && !options.force {
        return Err(Error::AlreadyInitialized { path: target_path });
    }
    let overlay_root = target_path
        .parent()
        .map(std::path::Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".castra");

    let diagnostics = Vec::new();
    let mut events = Vec::new();

    {
        let mut reporter = ReporterProxy::new(reporter, &mut events);

        if let Some(parent) = target_path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent).map_err(|source| Error::CreateDir {
                path: parent.to_path_buf(),
                source,
            })?;
        }

        std::fs::create_dir_all(&overlay_root).map_err(|source| Error::CreateDir {
            path: overlay_root.clone(),
            source,
        })?;

        let config_contents = default_config_contents(&project_name);
        std::fs::write(&target_path, config_contents).map_err(|source| Error::WriteConfig {
            path: target_path.clone(),
            source,
        })?;

        reporter.emit(Event::Message {
            severity: Severity::Info,
            text: "Created castra project scaffold.".to_string(),
        });
    }

    Ok(OperationOutput::new(InitOutcome {
        config_path: target_path,
        project_name,
        state_root,
        overlay_root,
        did_overwrite: target_exists && options.force,
    })
    .with_diagnostics(diagnostics)
    .with_events(events))
}

pub fn up(options: UpOptions, reporter: Option<&mut dyn Reporter>) -> OperationResult<UpOutcome> {
    let mut diagnostics = Vec::new();
    let mut events = Vec::new();

    let (mut project, _) = load_project_for_operation(&options.config, &mut diagnostics)?;
    apply_bootstrap_overrides(&mut project, &options.bootstrap)?;

    let outcome = {
        let mut reporter = ReporterProxy::new(reporter, &mut events);

        let status_core::StatusSnapshot {
            diagnostics: mut status_diags,
            rows: status_rows,
            ..
        } = status_core::collect_status(&project);
        diagnostics.append(&mut status_diags);

        let running: Vec<String> = status_rows
            .iter()
            .filter(|row| row.state == "running")
            .map(|row| row.name.clone())
            .collect();
        if !running.is_empty() {
            return Err(Error::PreflightFailed {
                message: format!(
                    "VMs already running: {}. Use `castra status` or `castra down` before invoking `up` again.",
                    running.join(", ")
                ),
            });
        }

        process_check(
            check_host_capacity(&project),
            options.force,
            &mut diagnostics,
            "Host resource checks failed:",
            "Rerun with `castra up --force` to override.",
        )?;

        let context = prepare_runtime_context(&project)?;

        process_check(
            check_disk_space(&project, &context),
            options.force,
            &mut diagnostics,
            "Insufficient free disk space:",
            "Rerun with `castra up --force` to override.",
        )?;

        ensure_ports_available(&project)?;

        let mut preparations = Vec::new();
        for vm in &project.vms {
            let prep = ensure_vm_assets(vm, &context)?;
            if let Some(managed) = &prep.managed {
                emit_managed_acquisition_events(
                    &mut reporter,
                    &context,
                    vm,
                    managed,
                    prep.assets.boot.as_ref(),
                );
            }
            if let Some(bytes) = prep.overlay_reclaimed_bytes {
                reporter.emit(Event::EphemeralLayerDiscarded {
                    vm: vm.name.clone(),
                    overlay_path: vm.overlay.clone(),
                    reclaimed_bytes: bytes,
                    reason: EphemeralCleanupReason::Orphan,
                });
            }
            if prep.overlay_created {
                reporter.emit(Event::OverlayPrepared {
                    vm: vm.name.clone(),
                    overlay_path: vm.overlay.clone(),
                });
            }
            preparations.push(prep);
        }

        let broker_outcome = reporter
            .with_event_buffer(|events| start_broker(&project, &context, &mut diagnostics, events))?
            .map(|pid| BrokerLaunchOutcome {
                pid,
                config: project.broker.clone(),
            });

        let mut launched_vms = Vec::new();
        for (vm, prep) in project.vms.iter().zip(preparations.iter()) {
            let pid = reporter
                .with_event_buffer(|events| launch_vm(vm, &prep.assets, &context, events))?;
            let assets = if let Some(managed) = &prep.managed {
                ManagedVmAssets {
                    managed_spec: Some(ManagedImageSpecHandle::from(managed.spec)),
                    managed_paths: Some(managed.paths.clone()),
                }
            } else {
                ManagedVmAssets {
                    managed_spec: None,
                    managed_paths: None,
                }
            };
            launched_vms.push(VmLaunchOutcome {
                name: vm.name.clone(),
                pid,
                assets,
                overlay_created: prep.overlay_created,
            });
        }

        reporter.emit(Event::Message {
            severity: Severity::Info,
            text: format!("Launched {} VM(s).", launched_vms.len()),
        });

        reporter.emit(Event::Message {
            severity: Severity::Info,
            text: "Guest disk changes are ephemeral; export via SSH before running `castra down` if you need to retain data.".to_string(),
        });

        let bootstrap_runs = bootstrap::run_all(
            &project,
            &context,
            &preparations,
            &mut reporter,
            &mut diagnostics,
        )?;

        if !bootstrap_runs.is_empty() {
            let success = bootstrap_runs
                .iter()
                .filter(|run| matches!(run.status, BootstrapRunStatus::Success))
                .count();
            let noop = bootstrap_runs
                .iter()
                .filter(|run| matches!(run.status, BootstrapRunStatus::NoOp))
                .count();
            let skipped = bootstrap_runs
                .iter()
                .filter(|run| matches!(run.status, BootstrapRunStatus::Skipped))
                .count();
            reporter.emit(Event::Message {
                severity: Severity::Info,
                text: format!(
                    "Bootstrap pipeline: {success} succeeded, {noop} up-to-date, {skipped} skipped.",
                ),
            });
        }

        reporter.emit(Event::Message {
            severity: Severity::Info,
            text: "Use `castra status` to monitor startup progress.".to_string(),
        });

        UpOutcome {
            state_root: context.state_root.clone(),
            log_root: context.log_root.clone(),
            launched_vms,
            broker: broker_outcome,
            bootstraps: bootstrap_runs,
        }
    };

    Ok(OperationOutput::new(outcome)
        .with_diagnostics(diagnostics)
        .with_events(events))
}

pub fn down(
    options: DownOptions,
    reporter: Option<&mut dyn Reporter>,
) -> OperationResult<DownOutcome> {
    let mut diagnostics = Vec::new();
    let mut events = Vec::new();
    let mut reporter = ReporterProxy::new(reporter, &mut events);

    let (project, _) = load_project_for_operation(&options.config, &mut diagnostics)?;
    let state_root = config_state_root(&project);
    let shutdown_timeouts = ShutdownTimeouts::new(
        options
            .graceful_wait
            .unwrap_or_else(|| project.lifecycle.graceful_wait()),
        options
            .sigterm_wait
            .unwrap_or_else(|| project.lifecycle.sigterm_wait()),
        options
            .sigkill_wait
            .unwrap_or_else(|| project.lifecycle.sigkill_wait()),
    );

    struct VmShutdownThreadResult {
        index: usize,
        name: String,
        changed: bool,
        outcome: ShutdownOutcome,
        diagnostics: Vec<Diagnostic>,
    }

    let (event_tx, event_rx) = mpsc::channel::<Event>();
    let mut handles = Vec::new();

    for (index, vm) in project.vms.iter().cloned().enumerate() {
        let tx_clone = event_tx.clone();
        let vm_name = vm.name.clone();
        let vm_state_root = state_root.clone();
        handles.push(thread::spawn(move || -> Result<VmShutdownThreadResult> {
            let report = shutdown_vm(&vm, &vm_state_root, shutdown_timeouts, Some(&tx_clone))?;

            Ok(VmShutdownThreadResult {
                index,
                name: vm_name,
                changed: report.changed,
                outcome: report.outcome,
                diagnostics: report.diagnostics,
            })
        }));
    }
    drop(event_tx);

    while let Ok(event) = event_rx.recv() {
        reporter.emit(event);
    }

    let mut first_error: Option<Error> = None;
    let mut vm_slots: Vec<Option<VmShutdownOutcome>> = vec![None; project.vms.len()];

    for handle in handles {
        match handle.join() {
            Ok(Ok(result)) => {
                diagnostics.extend(result.diagnostics);
                vm_slots[result.index] = Some(VmShutdownOutcome {
                    name: result.name,
                    changed: result.changed,
                    outcome: result.outcome,
                });
            }
            Ok(Err(err)) => {
                if first_error.is_none() {
                    first_error = Some(err);
                }
            }
            Err(payload) => panic::resume_unwind(payload),
        }
    }

    if let Some(err) = first_error {
        return Err(err);
    }

    let vm_results = vm_slots
        .into_iter()
        .map(|slot| {
            slot.unwrap_or_else(|| {
                panic!("shutdown worker did not produce a result for configured VM")
            })
        })
        .collect::<Vec<_>>();

    let broker_changed = reporter
        .with_event_buffer(|events| shutdown_broker(&state_root, events, &mut diagnostics))?;

    let outcome = DownOutcome {
        vm_results,
        broker: BrokerShutdownOutcome {
            changed: broker_changed,
        },
    };

    let any_vm = outcome.vm_results.iter().any(|vm| vm.changed);
    match (any_vm, outcome.broker.changed) {
        (false, false) => reporter.emit(Event::Message {
            severity: Severity::Info,
            text: "No running VMs or broker detected.".to_string(),
        }),
        (true, false) => reporter.emit(Event::Message {
            severity: Severity::Info,
            text: "All VMs have been stopped.".to_string(),
        }),
        (false, true) => reporter.emit(Event::Message {
            severity: Severity::Info,
            text: "Broker listener stopped.".to_string(),
        }),
        (true, true) => {
            reporter.emit(Event::Message {
                severity: Severity::Info,
                text: "All VMs have been stopped.".to_string(),
            });
            reporter.emit(Event::Message {
                severity: Severity::Info,
                text: "Broker listener stopped.".to_string(),
            });
        }
    }

    Ok(OperationOutput::new(outcome)
        .with_diagnostics(diagnostics)
        .with_events(events))
}

pub fn status(
    options: StatusOptions,
    _reporter: Option<&mut dyn Reporter>,
) -> OperationResult<StatusOutcome> {
    let mut diagnostics = Vec::new();
    let (project, _) = load_project_for_operation(&options.config, &mut diagnostics)?;

    let status_core::StatusSnapshot {
        rows,
        broker_state: broker_state_raw,
        diagnostics: mut status_diags,
        reachable,
        last_handshake,
    } = status_core::collect_status(&project);
    diagnostics.append(&mut status_diags);

    let broker_state = match broker_state_raw {
        BrokerProcessState::Running { pid } => super::outcome::BrokerState::Running { pid },
        BrokerProcessState::Offline => super::outcome::BrokerState::Offline,
    };

    let (last_handshake_vm, last_handshake_age_ms) = match last_handshake {
        Some(handshake) => {
            let age_ms = handshake.age.as_millis().min(u128::from(u64::MAX)) as u64;
            (Some(handshake.vm), Some(age_ms))
        }
        None => (None, None),
    };

    let outcome = StatusOutcome {
        project_path: project.file_path.clone(),
        project_name: project.project_name.clone(),
        config_version: project.version.clone(),
        broker_port: project.broker.port,
        broker_state,
        reachable,
        last_handshake_vm,
        last_handshake_age_ms,
        rows,
    };

    Ok(OperationOutput::new(outcome).with_diagnostics(diagnostics))
}

pub fn ports(
    options: PortsOptions,
    _reporter: Option<&mut dyn Reporter>,
) -> OperationResult<PortsOutcome> {
    let mut diagnostics = Vec::new();
    let (project, _) = load_project_for_operation(&options.config, &mut diagnostics)?;

    let (outcome, mut port_diags) = ports_core::summarize(&project, options.view);
    diagnostics.append(&mut port_diags);

    Ok(OperationOutput::new(outcome).with_diagnostics(diagnostics))
}

pub fn logs(
    options: LogsOptions,
    _reporter: Option<&mut dyn Reporter>,
) -> OperationResult<LogsOutcome> {
    let mut diagnostics = Vec::new();
    let (project, _) = load_project_for_operation(&options.config, &mut diagnostics)?;

    let outcome = logs_core::collect_logs(&project, options.tail, options.follow)?;

    Ok(OperationOutput::new(outcome).with_diagnostics(diagnostics))
}

pub fn clean(
    options: CleanOptions,
    reporter: Option<&mut dyn Reporter>,
) -> OperationResult<CleanOutcome> {
    clean::clean(options, reporter)
}

pub fn broker(options: BrokerOptions, _reporter: Option<&mut dyn Reporter>) -> OperationResult<()> {
    broker_core::run(&options)?;
    Ok(OperationOutput::new(()))
}

pub(super) fn load_project_for_operation(
    options: &ConfigLoadOptions,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<(ProjectConfig, bool)> {
    let ProjectLoad {
        config,
        diagnostics: diag,
        synthetic,
    } = load_project(options)?;
    diagnostics.extend(diag);
    Ok((config, synthetic))
}

fn apply_bootstrap_overrides(
    project: &mut ProjectConfig,
    overrides: &BootstrapOverrides,
) -> Result<()> {
    if overrides.global.is_none() && overrides.per_vm.is_empty() {
        return Ok(());
    }

    if let Some(mode) = overrides.global {
        project.bootstrap.mode = mode;
        for vm in &mut project.vms {
            vm.bootstrap.mode = mode;
        }
    }

    if overrides.per_vm.is_empty() {
        return Ok(());
    }

    for (vm_name, mode) in &overrides.per_vm {
        match project.vms.iter_mut().find(|vm| vm.name == *vm_name) {
            Some(vm) => {
                vm.bootstrap.mode = *mode;
            }
            None => {
                let available = project
                    .vms
                    .iter()
                    .map(|vm| vm.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ");
                return Err(Error::PreflightFailed {
                    message: format!(
                        "Bootstrap override references unknown VM `{vm_name}`. Available VMs: {available}."
                    ),
                });
            }
        }
    }

    Ok(())
}

fn emit_managed_acquisition_events(
    reporter: &mut ReporterProxy<'_, '_>,
    context: &RuntimeContext,
    vm: &VmDefinition,
    managed: &ManagedAcquisition,
    boot: Option<&BootOverrides>,
) {
    let handle = ManagedImageSpecHandle::from(managed.spec);
    let image_id = managed.spec.id.to_string();
    let image_version = managed.spec.version.to_string();
    let image_path = managed.paths.root_disk.clone();

    for event in &managed.events {
        reporter.emit(Event::ManagedArtifact {
            spec: handle.clone(),
            artifact: event.artifact,
            detail: event.detail.clone(),
            text: event.message.clone(),
        });
    }

    let verification_plan: Vec<ManagedImageArtifactPlan> = managed
        .verification
        .plan
        .iter()
        .map(|expectation| build_managed_artifact_plan(expectation, &managed.paths))
        .collect();
    reporter.emit(Event::ManagedImageVerificationStarted {
        image_id: image_id.clone(),
        image_version: image_version.clone(),
        image_path: image_path.clone(),
        started_at: managed.verification.started_at,
        plan: verification_plan,
    });

    let verification_artifacts: Vec<ManagedImageArtifactReport> = managed
        .verification
        .artifacts
        .iter()
        .map(|summary| build_managed_artifact_report(summary, &managed.paths))
        .collect();
    let mut total_size_bytes = 0u64;
    for artifact in &verification_artifacts {
        if let Some(size) = artifact.size_bytes {
            total_size_bytes = total_size_bytes.saturating_add(size);
        }
    }
    let verification_error = match &managed.verification.outcome {
        ManagedImageVerificationOutcome::Failure { reason } => Some(reason.clone()),
        _ => None,
    };
    reporter.emit(Event::ManagedImageVerificationResult {
        image_id: image_id.clone(),
        image_version: image_version.clone(),
        image_path: image_path.clone(),
        completed_at: managed.verification.completed_at,
        duration_ms: managed.verification.duration.as_millis() as u64,
        outcome: managed.verification.outcome.clone(),
        error: verification_error,
        size_bytes: total_size_bytes,
        artifacts: verification_artifacts,
    });

    if let Some(boot) = boot {
        let profile_id = managed_profile_id(managed.spec);
        let profile_steps = build_profile_steps(boot);
        let profile_started_at = SystemTime::now();
        let profile_timer = Instant::now();
        context.image_manager.log_profile_application_started(
            managed.spec,
            &vm.name,
            boot.kernel.as_path(),
            boot.initrd.as_deref(),
            &boot.append,
            &boot.extra_args,
            boot.machine.as_deref(),
            profile_started_at,
        );
        reporter.emit(Event::ManagedImageProfileApplied {
            image_id: image_id.clone(),
            image_version: image_version.clone(),
            vm: vm.name.clone(),
            profile_id: profile_id.clone(),
            started_at: profile_started_at,
            steps: profile_steps.clone(),
        });
        let profile_outcome = ManagedImageProfileOutcome::Applied;
        let profile_duration = profile_timer.elapsed();
        context.image_manager.log_profile_application(
            managed.spec,
            &vm.name,
            boot.kernel.as_path(),
            boot.initrd.as_deref(),
            &boot.append,
            &boot.extra_args,
            boot.machine.as_deref(),
            profile_started_at,
            profile_duration,
            &profile_outcome,
        );
        let profile_completed_at = SystemTime::now();
        reporter.emit(Event::ManagedImageProfileResult {
            image_id: image_id.clone(),
            image_version: image_version.clone(),
            vm: vm.name.clone(),
            profile_id,
            completed_at: profile_completed_at,
            duration_ms: profile_duration.as_millis() as u64,
            outcome: profile_outcome,
            error: None,
            steps: profile_steps,
        });
    }
}

fn build_managed_artifact_plan(
    expectation: &ManagedImageArtifactExpectation,
    paths: &ManagedImagePaths,
) -> ManagedImageArtifactPlan {
    ManagedImageArtifactPlan {
        kind: expectation.kind,
        filename: expectation.filename.clone(),
        path: managed_artifact_path(paths, expectation.kind),
        expected_sha256: expectation.expected_sha256.clone(),
        expected_size_bytes: expectation.expected_size_bytes,
    }
}

fn build_managed_artifact_report(
    summary: &ManagedImageArtifactSummary,
    paths: &ManagedImagePaths,
) -> ManagedImageArtifactReport {
    ManagedImageArtifactReport {
        kind: summary.kind,
        filename: summary.filename.clone(),
        path: managed_artifact_path(paths, summary.kind),
        size_bytes: Some(summary.size_bytes),
        checksums: managed_artifact_checksums(summary),
    }
}

fn managed_artifact_path(paths: &ManagedImagePaths, kind: ManagedArtifactKind) -> Option<PathBuf> {
    match kind {
        ManagedArtifactKind::RootDisk => Some(paths.root_disk.clone()),
        ManagedArtifactKind::Kernel => paths.kernel.clone(),
        ManagedArtifactKind::Initrd => paths.initrd.clone(),
    }
}

fn managed_artifact_checksums(summary: &ManagedImageArtifactSummary) -> Vec<ManagedImageChecksum> {
    let mut checksums = Vec::new();
    checksums.push(ManagedImageChecksum {
        algo: "sha256".to_string(),
        value: summary.final_sha256.clone(),
    });
    if let Some(source) = &summary.source_sha256 {
        checksums.push(ManagedImageChecksum {
            algo: "source_sha256".to_string(),
            value: source.clone(),
        });
    }
    checksums
}

fn build_profile_steps(boot: &BootOverrides) -> Vec<String> {
    let mut steps = Vec::new();
    steps.push(format!("kernel={}", boot.kernel.display()));
    if let Some(initrd) = &boot.initrd {
        steps.push(format!("initrd={}", initrd.display()));
    }
    if !boot.append.trim().is_empty() {
        steps.push(format!("append={}", boot.append));
    }
    if !boot.extra_args.is_empty() {
        steps.push(format!("extra_args={}", boot.extra_args.join(" ")));
    }
    if let Some(machine) = &boot.machine {
        steps.push(format!("machine={}", machine));
    }
    steps
}

fn managed_profile_id(spec: &ManagedImageSpec) -> String {
    format!("{}@{}::boot", spec.id, spec.version)
}

fn process_check(
    outcome: CheckOutcome,
    force: bool,
    diagnostics: &mut Vec<Diagnostic>,
    header: &str,
    override_hint: &str,
) -> Result<()> {
    let CheckOutcome { warnings, failures } = outcome;
    diagnostics.extend(warnings);
    if failures.is_empty() {
        return Ok(());
    }

    if force {
        for failure in failures {
            diagnostics.push(Diagnostic::new(
                Severity::Warning,
                format!("{failure} (continuing due to --force)."),
            ));
        }
        Ok(())
    } else {
        let bullet_list = failures
            .iter()
            .map(|msg| format!("- {msg}"))
            .collect::<Vec<_>>()
            .join("\n");
        Err(Error::PreflightFailed {
            message: format!("{header}\n{bullet_list}\n{override_hint}"),
        })
    }
}

pub(super) struct ReporterProxy<'a, 'b> {
    delegate: Option<&'a mut dyn Reporter>,
    events: &'b mut Vec<Event>,
}

impl<'a, 'b> ReporterProxy<'a, 'b> {
    fn new(delegate: Option<&'a mut dyn Reporter>, events: &'b mut Vec<Event>) -> Self {
        Self { delegate, events }
    }

    fn emit(&mut self, event: Event) {
        self.events.push(event.clone());
        if let Some(reporter) = &mut self.delegate {
            reporter.report(event);
        }
    }

    fn with_event_buffer<F, T>(&mut self, f: F) -> T
    where
        F: FnOnce(&mut Vec<Event>) -> T,
    {
        let start_len = self.events.len();
        let result = f(self.events);
        if let Some(reporter) = &mut self.delegate {
            for event in self.events[start_len..].iter().cloned() {
                reporter.report(event);
            }
        }
        result
    }
}

impl Reporter for ReporterProxy<'_, '_> {
    fn report(&mut self, event: Event) {
        self.emit(event);
    }
}

pub fn bus_publish(
    options: BusPublishOptions,
    reporter: Option<&mut dyn Reporter>,
) -> OperationResult<BusPublishOutcome> {
    bus::publish(options, reporter)
}

pub fn bus_tail(
    options: BusTailOptions,
    reporter: Option<&mut dyn Reporter>,
) -> OperationResult<BusTailOutcome> {
    bus::tail(options, reporter)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DEFAULT_BROKER_PORT;
    use crate::config::{
        BaseImageSource, BootstrapConfig, BootstrapMode, BrokerConfig,
        DEFAULT_BOOTSTRAP_HANDSHAKE_WAIT_SECS, LifecycleConfig, ManagedDiskKind,
        ManagedImageReference, MemorySpec, ProjectConfig, VmBootstrapConfig, VmDefinition,
        Workflows,
    };
    use crate::core::reporter::Reporter;
    use crate::core::runtime::{ManagedAcquisition, RuntimeContext};
    use crate::managed::{
        ArtifactSource, ImageManager, ManagedArtifactEvent, ManagedArtifactEventDetail,
        ManagedArtifactKind, ManagedArtifactSpec, ManagedImageArtifactExpectation,
        ManagedImageArtifactSummary, ManagedImagePaths, ManagedImageSpec, ManagedImageVerification,
        ManagedImageVerificationOutcome, QemuProfile,
    };
    use serde_json::Value;
    use std::collections::HashMap;
    use std::error::Error as StdError;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{Duration, UNIX_EPOCH};
    use tempfile::tempdir;

    fn sample_vm(project_root: &Path, name: &str, mode: BootstrapMode) -> VmDefinition {
        let bootstrap_dir = project_root.join("bootstrap").join(name);
        VmDefinition {
            name: name.to_string(),
            role_name: name.to_string(),
            replica_index: 0,
            description: None,
            base_image: BaseImageSource::Path(PathBuf::from("/tmp/base.qcow2")),
            overlay: PathBuf::from(format!("/tmp/{name}.qcow2")),
            cpus: 1,
            memory: MemorySpec::new("512 MiB", Some(512 * 1024 * 1024)),
            port_forwards: Vec::new(),
            bootstrap: VmBootstrapConfig {
                mode,
                script: Some(bootstrap_dir.join("run.sh")),
                payload: Some(bootstrap_dir.join("payload")),
                handshake_timeout_secs: DEFAULT_BOOTSTRAP_HANDSHAKE_WAIT_SECS,
                remote_dir: PathBuf::from("/tmp/castra-bootstrap"),
                env: HashMap::new(),
                verify: None,
            },
        }
    }

    fn sample_project_with_modes(
        vm_modes: &[(&str, BootstrapMode)],
        default_mode: BootstrapMode,
    ) -> ProjectConfig {
        let project_root = PathBuf::from("/tmp/project");
        let vms = vm_modes
            .iter()
            .map(|(name, mode)| sample_vm(&project_root, name, *mode))
            .collect();

        ProjectConfig {
            file_path: PathBuf::from("castra.toml"),
            project_root: project_root.clone(),
            version: "0.1.0".to_string(),
            project_name: "demo".to_string(),
            vms,
            state_root: PathBuf::from("/tmp/castra"),
            workflows: Workflows { init: Vec::new() },
            broker: BrokerConfig {
                port: DEFAULT_BROKER_PORT,
            },
            lifecycle: LifecycleConfig::default(),
            bootstrap: BootstrapConfig {
                mode: default_mode,
                handshake_timeout_secs: DEFAULT_BOOTSTRAP_HANDSHAKE_WAIT_SECS,
                remote_dir: PathBuf::from("/tmp/castra-bootstrap"),
                env: HashMap::new(),
            },
            warnings: Vec::new(),
        }
    }

    #[test]
    fn apply_bootstrap_overrides_sets_global_mode() {
        let mut project = sample_project_with_modes(
            &[
                ("api-0", BootstrapMode::Auto),
                ("api-1", BootstrapMode::Always),
            ],
            BootstrapMode::Auto,
        );

        let overrides = BootstrapOverrides {
            global: Some(BootstrapMode::Disabled),
            per_vm: HashMap::new(),
        };

        apply_bootstrap_overrides(&mut project, &overrides).expect("apply overrides");

        assert_eq!(project.bootstrap.mode, BootstrapMode::Disabled);
        for vm in &project.vms {
            assert_eq!(vm.bootstrap.mode, BootstrapMode::Disabled);
        }
    }

    #[test]
    fn apply_bootstrap_overrides_sets_vm_specific_mode() {
        let mut project = sample_project_with_modes(
            &[
                ("api-0", BootstrapMode::Auto),
                ("api-1", BootstrapMode::Auto),
            ],
            BootstrapMode::Auto,
        );

        let mut per_vm = HashMap::new();
        per_vm.insert("api-1".to_string(), BootstrapMode::Always);
        let overrides = BootstrapOverrides {
            global: Some(BootstrapMode::Disabled),
            per_vm,
        };

        apply_bootstrap_overrides(&mut project, &overrides).expect("apply overrides");

        let api0 = project.vms.iter().find(|vm| vm.name == "api-0").unwrap();
        let api1 = project.vms.iter().find(|vm| vm.name == "api-1").unwrap();

        assert_eq!(project.bootstrap.mode, BootstrapMode::Disabled);
        assert_eq!(api0.bootstrap.mode, BootstrapMode::Disabled);
        assert_eq!(api1.bootstrap.mode, BootstrapMode::Always);
    }

    #[test]
    fn apply_bootstrap_overrides_errors_for_unknown_vm() {
        let mut project =
            sample_project_with_modes(&[("api-0", BootstrapMode::Auto)], BootstrapMode::Auto);

        let mut per_vm = HashMap::new();
        per_vm.insert("missing".to_string(), BootstrapMode::Always);
        let overrides = BootstrapOverrides {
            global: None,
            per_vm,
        };

        let err = apply_bootstrap_overrides(&mut project, &overrides).unwrap_err();
        match err {
            Error::PreflightFailed { message } => {
                assert!(
                    message.contains("unknown VM `missing`"),
                    "unexpected message: {message}"
                );
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[derive(Default)]
    struct RecordingReporter {
        events: Vec<Event>,
    }

    impl Reporter for RecordingReporter {
        fn report(&mut self, event: Event) {
            self.events.push(event);
        }
    }

    #[test]
    fn reporter_proxy_forwards_events_to_delegate() {
        let mut delegate = RecordingReporter::default();
        let mut events = Vec::new();

        {
            let mut proxy = ReporterProxy::new(Some(&mut delegate), &mut events);
            proxy.emit(Event::Message {
                severity: Severity::Info,
                text: "hello".to_string(),
            });
            proxy.with_event_buffer(|buffer| {
                buffer.push(Event::Message {
                    severity: Severity::Warning,
                    text: "warn".to_string(),
                });
            });
        }

        assert_eq!(events.len(), 2);
        assert_eq!(delegate.events.len(), 2);

        for (buffered, reported) in events.iter().zip(delegate.events.iter()) {
            assert_eq!(format!("{buffered:?}"), format!("{reported:?}"));
        }
    }

    #[test]
    fn managed_acquisition_events_align_across_sinks() -> std::result::Result<(), Box<dyn StdError>>
    {
        let temp = tempdir()?;
        let state_root = temp.path().join("state");
        let log_root = temp.path().join("logs");
        let storage_root = temp.path().join("images");
        fs::create_dir_all(&state_root)?;
        fs::create_dir_all(&log_root)?;
        fs::create_dir_all(&storage_root)?;

        let image_manager = ImageManager::new(storage_root, log_root.clone(), None);
        let context = RuntimeContext {
            state_root: state_root.clone(),
            log_root: log_root.clone(),
            qemu_system: PathBuf::from("/bin/true"),
            qemu_img: None,
            image_manager,
            accelerators: Vec::new(),
        };

        let artifacts_box = Box::new([ManagedArtifactSpec {
            kind: ManagedArtifactKind::RootDisk,
            final_filename: "disk.qcow2",
            source: ArtifactSource {
                url: "https://example.invalid/disk",
                sha256: Some("expected"),
                size: Some(12),
            },
            transformations: &[],
        }]);
        let artifacts_static: &'static [ManagedArtifactSpec] = Box::leak(artifacts_box);
        let spec = Box::leak(Box::new(ManagedImageSpec {
            id: "test-image",
            version: "v1",
            artifacts: artifacts_static,
            qemu: QemuProfile {
                kernel: None,
                initrd: None,
                append: "",
                machine: None,
                extra_args: &[],
            },
        }));

        let plan_expectations = vec![ManagedImageArtifactExpectation {
            kind: ManagedArtifactKind::RootDisk,
            filename: "disk.qcow2".to_string(),
            expected_sha256: Some("expected".to_string()),
            expected_size_bytes: Some(12),
        }];
        let verification_summary = ManagedImageArtifactSummary {
            kind: ManagedArtifactKind::RootDisk,
            filename: "disk.qcow2".to_string(),
            size_bytes: 12,
            final_sha256: "final".to_string(),
            source_sha256: Some("expected".to_string()),
        };
        let verification = ManagedImageVerification {
            plan: plan_expectations.clone(),
            artifacts: vec![verification_summary.clone()],
            started_at: UNIX_EPOCH + Duration::from_secs(5),
            completed_at: UNIX_EPOCH + Duration::from_secs(8),
            duration: Duration::from_secs(3),
            outcome: ManagedImageVerificationOutcome::Success,
        };

        let root_disk_path = temp.path().join("disk.qcow2");
        fs::write(&root_disk_path, b"disk")?;
        let kernel_path = temp.path().join("kernel");
        fs::write(&kernel_path, b"kernel")?;
        let initrd_path = temp.path().join("initrd");
        fs::write(&initrd_path, b"initrd")?;
        let managed_paths = ManagedImagePaths {
            root_disk: root_disk_path.clone(),
            kernel: Some(kernel_path.clone()),
            initrd: Some(initrd_path.clone()),
        };

        let managed_event = ManagedArtifactEvent {
            artifact: ManagedArtifactKind::RootDisk,
            detail: ManagedArtifactEventDetail::CacheHit,
            message: "root disk: cache hit (verified).".to_string(),
        };
        let managed = ManagedAcquisition {
            spec,
            events: vec![managed_event],
            verification: verification.clone(),
            paths: managed_paths.clone(),
        };

        let boot = BootOverrides {
            kernel: kernel_path.clone(),
            initrd: Some(initrd_path.clone()),
            append: "console=ttyS0".to_string(),
            extra_args: vec!["debug".to_string()],
            machine: Some("pc-q35".to_string()),
        };

        let bootstrap_dir = temp.path().join("bootstrap").join("vm-test");
        let vm = VmDefinition {
            name: "vm-test".to_string(),
            role_name: "app".to_string(),
            replica_index: 0,
            description: None,
            base_image: BaseImageSource::Managed(ManagedImageReference {
                name: spec.id.to_string(),
                version: spec.version.to_string(),
                disk: ManagedDiskKind::RootDisk,
                checksum: None,
                size_bytes: None,
            }),
            overlay: temp.path().join("vm-test.qcow2"),
            cpus: 1,
            memory: MemorySpec::new("512M", Some(512 * 1024 * 1024)),
            port_forwards: Vec::new(),
            bootstrap: VmBootstrapConfig {
                mode: BootstrapMode::Auto,
                script: Some(bootstrap_dir.join("run.sh")),
                payload: Some(bootstrap_dir.join("payload")),
                handshake_timeout_secs: DEFAULT_BOOTSTRAP_HANDSHAKE_WAIT_SECS,
                remote_dir: PathBuf::from("/tmp/castra-bootstrap"),
                env: HashMap::new(),
                verify: None,
            },
        };

        context.image_manager.log_verification_started(
            spec,
            &plan_expectations,
            verification.started_at,
        );
        context
            .image_manager
            .log_verification_result(spec, &verification);

        let mut delegate = RecordingReporter::default();
        let mut events = Vec::new();
        {
            let mut proxy = ReporterProxy::new(Some(&mut delegate), &mut events);
            emit_managed_acquisition_events(&mut proxy, &context, &vm, &managed, Some(&boot));
        }

        assert_eq!(events.len(), delegate.events.len());
        for (buffered, reported) in events.iter().zip(delegate.events.iter()) {
            assert_eq!(format!("{buffered:?}"), format!("{reported:?}"));
        }

        let log_path = context.log_root.join("image-manager.log");
        let contents = fs::read_to_string(&log_path)?;
        let lines: Vec<&str> = contents.lines().collect();
        assert_eq!(lines.len(), 4);

        let verification_started: Value = serde_json::from_str(lines[0])?;
        assert_eq!(
            verification_started["event"],
            "managed-image-verification-started"
        );
        assert_eq!(verification_started["image"], spec.id);
        assert_eq!(verification_started["version"], spec.version);
        assert_eq!(verification_started["plan"][0]["filename"], "disk.qcow2");

        let verification_result: Value = serde_json::from_str(lines[1])?;
        assert_eq!(
            verification_result["event"],
            "managed-image-verification-result"
        );
        assert_eq!(verification_result["outcome"], "success");
        assert_eq!(
            verification_result["artifacts"][0]["filename"],
            "disk.qcow2"
        );

        let profile_started: Value = serde_json::from_str(lines[2])?;
        assert_eq!(profile_started["event"], "managed-image-profile-applied");
        assert_eq!(profile_started["vm"], vm.name);

        let profile_result: Value = serde_json::from_str(lines[3])?;
        assert_eq!(profile_result["event"], "managed-image-profile-result");
        assert_eq!(profile_result["vm"], vm.name);
        assert_eq!(profile_result["outcome"], "applied");

        assert!(
            events
                .iter()
                .any(|event| matches!(event, Event::ManagedArtifact { .. }))
        );
        let (image_id, plan) = events
            .iter()
            .find_map(|event| match event {
                Event::ManagedImageVerificationStarted { image_id, plan, .. } => {
                    Some((image_id, plan))
                }
                _ => None,
            })
            .expect("verification started event");
        assert_eq!(image_id, spec.id);
        assert_eq!(
            plan[0].path.as_ref().map(Path::new),
            Some(managed.paths.root_disk.as_path())
        );

        let (size_bytes, artifact_reports) = events
            .iter()
            .find_map(|event| match event {
                Event::ManagedImageVerificationResult {
                    size_bytes,
                    artifacts,
                    ..
                } => Some((size_bytes, artifacts)),
                _ => None,
            })
            .expect("verification result event");
        assert_eq!(*size_bytes, 12);
        assert_eq!(artifact_reports[0].filename, "disk.qcow2");

        let profile_steps = events
            .iter()
            .find_map(|event| match event {
                Event::ManagedImageProfileResult { steps, .. } => Some(steps),
                _ => None,
            })
            .expect("profile result event");
        assert!(profile_steps.iter().any(|step| step.starts_with("kernel=")));
        assert!(
            profile_steps
                .iter()
                .any(|step| step.contains("console=ttyS0"))
        );

        Ok(())
    }
}
