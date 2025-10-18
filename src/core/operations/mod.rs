use std::panic;
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;

mod bus;
mod clean;

use super::bootstrap;
use super::broker as broker_core;
use super::diagnostics::{Diagnostic, Severity};
use super::events::{EphemeralCleanupReason, Event, ShutdownOutcome};
use super::logs as logs_core;
use super::options::{
    BootstrapOverrides, BrokerOptions, BusPublishOptions, BusTailOptions, CleanOptions,
    ConfigLoadOptions, DownOptions, InitOptions, LogsOptions, PortsOptions, StatusOptions,
    UpOptions,
};
use super::outcome::{
    BootstrapRunStatus, BrokerLaunchOutcome, BrokerShutdownOutcome, BusPublishOutcome,
    BusTailOutcome, CleanOutcome, DownOutcome, InitOutcome, LogsOutcome, OperationOutput,
    OperationResult, PortsOutcome, StatusOutcome, UpOutcome, VmLaunchOutcome, VmShutdownOutcome,
};
use super::ports as ports_core;
use super::project::{
    ProjectLoad, config_state_root, default_config_contents, default_project_name, load_project,
    preferred_init_target,
};
use super::reporter::Reporter;
use super::runtime::{
    BrokerProcessState, CheckOutcome, ShutdownTimeouts, check_disk_space, check_host_capacity,
    ensure_ports_available, ensure_vm_assets, launch_vm, prepare_runtime_context, shutdown_broker,
    shutdown_vm, start_broker,
};
use super::status as status_core;
use crate::config::{self, ProjectConfig};
use crate::error::{Error, Result};

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

    let mut reporter_proxy = ReporterProxy::new(reporter, &mut events);

    if options.plan {
        let state_root = config_state_root(&project);
        let log_root = state_root.join("logs");
        let plans = bootstrap::plan_all(&project, &mut reporter_proxy, &mut diagnostics)?;
        reporter_proxy.emit(Event::Message {
            severity: Severity::Info,
            text: "Plan mode only – no VMs were launched.".to_string(),
        });
        return Ok(OperationOutput::new(UpOutcome {
            state_root,
            log_root,
            launched_vms: Vec::new(),
            broker: None,
            bootstraps: Vec::new(),
            plans,
        })
        .with_diagnostics(diagnostics)
        .with_events(events));
    }

    let outcome = {
        let mut reporter = reporter_proxy;

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
            for event in prep.events.iter().cloned() {
                reporter.emit(event);
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
            launched_vms.push(VmLaunchOutcome {
                name: vm.name.clone(),
                pid,
                base_image: vm.base_image.path().to_path_buf(),
                base_image_provenance: vm.base_image.provenance(),
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
            plans: Vec::new(),
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
