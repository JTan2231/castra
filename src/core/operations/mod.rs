use std::path::PathBuf;

mod clean;

use crate::config::{self, ProjectConfig};
use crate::error::{Error, Result};

use super::broker as broker_core;
use super::diagnostics::{Diagnostic, Severity};
use super::events::{Event, ManagedImageSpecHandle};
use super::logs as logs_core;
use super::options::{
    BrokerOptions, CleanOptions, ConfigLoadOptions, DownOptions, InitOptions, LogsOptions,
    PortsOptions, StatusOptions, UpOptions,
};
use super::outcome::{
    BrokerLaunchOutcome, BrokerShutdownOutcome, CleanOutcome, DownOutcome, InitOutcome,
    LogsOutcome, ManagedVmAssets, OperationOutput, OperationResult, PortsOutcome, StatusOutcome,
    UpOutcome, VmLaunchOutcome, VmShutdownOutcome,
};
use super::ports as ports_core;
use super::project::{
    ProjectLoad, config_state_root, default_config_contents, default_project_name, load_project,
    preferred_init_target,
};
use super::reporter::Reporter;
use super::runtime::{
    BrokerProcessState, CheckOutcome, check_disk_space, check_host_capacity,
    ensure_ports_available, ensure_vm_assets, launch_vm, prepare_runtime_context, shutdown_broker,
    shutdown_vm, start_broker,
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

    let (project, _) = load_project_for_operation(&options.config, &mut diagnostics)?;

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
                let handle = ManagedImageSpecHandle::from(managed.spec);
                if prep.assets.boot.is_some() {
                    let profile_label = if prep
                        .assets
                        .boot
                        .as_ref()
                        .and_then(|boot| boot.initrd.as_ref())
                        .is_some()
                    {
                        "kernel/initrd"
                    } else {
                        "kernel"
                    };
                    reporter.emit(Event::Message {
                        severity: Severity::Info,
                        text: format!(
                            "→ {}@{}: applied boot profile ({}) for VM `{}`.",
                            handle.id, handle.version, profile_label, vm.name
                        ),
                    });
                }
                for event in &managed.events {
                    reporter.emit(Event::ManagedArtifact {
                        spec: handle.clone(),
                        text: event.message.clone(),
                    });
                }
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
            text: "Use `castra status` to monitor startup progress.".to_string(),
        });

        UpOutcome {
            state_root: context.state_root.clone(),
            log_root: context.log_root.clone(),
            launched_vms,
            broker: broker_outcome,
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

    let mut vm_results = Vec::new();
    for vm in &project.vms {
        let changed = reporter
            .with_event_buffer(|events| shutdown_vm(vm, &state_root, events, &mut diagnostics))?;
        vm_results.push(VmShutdownOutcome {
            name: vm.name.clone(),
            changed,
        });
    }

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
