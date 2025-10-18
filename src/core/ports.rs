use std::collections::{HashMap, HashSet};
use std::io;
use std::net::{TcpListener, UdpSocket};

use crate::config::{PortForward, PortProtocol, ProjectConfig};

use super::diagnostics::{Diagnostic, Severity};
use super::options::PortsView;
use super::outcome::{
    PortConflictRow, PortForwardRow, PortForwardStatus, PortInactiveReason, PortsOutcome,
    VmPortDetail,
};
use super::project::config_state_root;
use super::runtime::inspect_vm_state;

type ForwardKey = (u16, u16, PortProtocol);

#[derive(Clone, Copy, PartialEq, Eq)]
enum VmRuntimeState {
    Running,
    Stopped,
    Unknown,
}

#[derive(Clone, Copy)]
enum ForwardRuntimeState {
    Active,
    Inactive(PortInactiveReason),
}

struct VmInspection {
    state: VmRuntimeState,
    forwards: HashMap<ForwardKey, ForwardRuntimeState>,
}

pub fn summarize(project: &ProjectConfig, view: PortsView) -> (PortsOutcome, Vec<Diagnostic>) {
    let mut diagnostics = Vec::new();
    let (conflicts, broker_collision) = project.port_conflicts();
    let conflict_ports: HashSet<u16> = conflicts.iter().map(|c| c.port).collect();
    let broker_conflict_port = broker_collision.as_ref().map(|c| c.port);

    let runtime_inspection = if matches!(view, PortsView::Active) {
        let (inspection, mut runtime_diags) = inspect_runtime_forwards(project);
        diagnostics.append(&mut runtime_diags);
        Some(inspection)
    } else {
        None
    };

    let mut declared = Vec::new();
    for vm in &project.vms {
        for forward in &vm.port_forwards {
            let mut status = if conflict_ports.contains(&forward.host) {
                PortForwardStatus::Conflicting
            } else if broker_conflict_port == Some(forward.host) {
                PortForwardStatus::BrokerReserved
            } else {
                PortForwardStatus::Declared
            };
            let mut inactive_reason = None;

            if matches!(view, PortsView::Active) && matches!(status, PortForwardStatus::Declared) {
                let key = forward_key(forward);
                if let Some(runtime) = runtime_inspection.as_ref() {
                    if let Some(vm_runtime) = runtime.get(&vm.name) {
                        match vm_runtime.state {
                            VmRuntimeState::Running => {
                                if let Some(forward_state) = vm_runtime.forwards.get(&key) {
                                    match forward_state {
                                        ForwardRuntimeState::Active => {
                                            status = PortForwardStatus::Active;
                                        }
                                        ForwardRuntimeState::Inactive(reason) => {
                                            inactive_reason = Some(*reason);
                                        }
                                    }
                                } else {
                                    inactive_reason =
                                        Some(PortInactiveReason::InspectionUnavailable);
                                }
                            }
                            VmRuntimeState::Stopped => {
                                inactive_reason = Some(PortInactiveReason::VmStopped);
                            }
                            VmRuntimeState::Unknown => {
                                inactive_reason = Some(PortInactiveReason::InspectionUnavailable);
                            }
                        }
                    } else {
                        inactive_reason = Some(PortInactiveReason::InspectionUnavailable);
                    }
                } else {
                    inactive_reason = Some(PortInactiveReason::InspectionUnavailable);
                }
            }

            declared.push(PortForwardRow {
                vm: vm.name.clone(),
                forward: forward.clone(),
                status,
                inactive_reason,
            });
        }
    }

    if let Some(collision) = broker_collision {
        diagnostics.push(
            Diagnostic::new(
                Severity::Warning,
                format!(
                    "Host port {} overlaps with the castra broker. Adjust the broker port or the forward.",
                    collision.port
                ),
            )
            .with_help("Update `[broker].port` or the conflicting `[[vms.port_forwards]]` entry."),
        );
    }

    let port_conflicts = conflicts
        .into_iter()
        .map(|conflict| PortConflictRow {
            port: conflict.port,
            vm_names: conflict.vm_names,
        })
        .collect::<Vec<_>>();

    diagnostics.extend(port_conflicts.iter().map(|conflict| {
        Diagnostic::new(
            Severity::Warning,
            format!(
                "Host port {} is declared by multiple VMs: {}.",
                conflict.port,
                conflict.vm_names.join(", ")
            ),
        )
    }));

    let vm_details = project
        .vms
        .iter()
        .map(|vm| VmPortDetail {
            name: vm.name.clone(),
            description: vm.description.clone(),
            base_image: vm.base_image.describe(),
            overlay: vm.overlay.clone(),
            cpus: vm.cpus,
            memory: vm.memory.original().to_string(),
            memory_bytes: vm.memory.bytes(),
            port_forwards: vm.port_forwards.clone(),
        })
        .collect();

    let without_forwards = ports_without_forwards(project);

    let outcome = PortsOutcome {
        project_path: project.file_path.clone(),
        project_name: project.project_name.clone(),
        config_version: project.version.clone(),
        broker_port: project.broker.port,
        declared,
        conflicts: port_conflicts,
        vm_details,
        without_forwards,
        view,
    };

    (outcome, diagnostics)
}

pub fn ports_without_forwards(project: &ProjectConfig) -> Vec<String> {
    project
        .vms
        .iter()
        .filter(|vm| vm.port_forwards.is_empty())
        .map(|vm| vm.name.clone())
        .collect()
}

fn forward_key(forward: &PortForward) -> ForwardKey {
    (forward.host, forward.guest, forward.protocol)
}

fn inspect_runtime_forwards(
    project: &ProjectConfig,
) -> (HashMap<String, VmInspection>, Vec<Diagnostic>) {
    let mut diagnostics = Vec::new();
    let mut inspections = HashMap::new();
    let state_root = config_state_root(project);

    for vm in &project.vms {
        let pidfile = state_root.join(format!("{}.pid", vm.name));
        let (state, _uptime, warnings) = inspect_vm_state(&pidfile, &vm.name);
        diagnostics.extend(
            warnings
                .into_iter()
                .map(|warning| Diagnostic::new(Severity::Warning, warning)),
        );

        let vm_state = match state.as_str() {
            "running" => VmRuntimeState::Running,
            "stopped" => VmRuntimeState::Stopped,
            _ => VmRuntimeState::Unknown,
        };

        let mut forwards = HashMap::new();
        if matches!(vm_state, VmRuntimeState::Running) {
            for forward in &vm.port_forwards {
                let key = forward_key(forward);
                match inspect_forward(forward) {
                    Ok(status) => {
                        forwards.insert(key, status);
                    }
                    Err(err) => {
                        diagnostics.push(
                            Diagnostic::new(
                                Severity::Info,
                                format!(
                                    "Runtime inspection for host port {} ({}) on `{}` is unavailable: {err}",
                                    forward.host,
                                    forward.protocol,
                                    vm.name
                                ),
                            )
                            .with_help(
                                "Showing declared status; inspect the forward manually if needed.",
                            ),
                        );
                        forwards.insert(
                            key,
                            ForwardRuntimeState::Inactive(
                                PortInactiveReason::InspectionUnavailable,
                            ),
                        );
                    }
                }
            }
        }

        inspections.insert(
            vm.name.clone(),
            VmInspection {
                state: vm_state,
                forwards,
            },
        );
    }

    (inspections, diagnostics)
}

fn inspect_forward(forward: &PortForward) -> io::Result<ForwardRuntimeState> {
    match forward.protocol {
        PortProtocol::Tcp => inspect_tcp_port(forward.host),
        PortProtocol::Udp => inspect_udp_port(forward.host),
    }
}

fn inspect_tcp_port(port: u16) -> io::Result<ForwardRuntimeState> {
    match TcpListener::bind(("127.0.0.1", port)) {
        Ok(listener) => {
            drop(listener);
            Ok(ForwardRuntimeState::Inactive(
                PortInactiveReason::PortNotBound,
            ))
        }
        Err(err) if err.kind() == io::ErrorKind::AddrInUse => Ok(ForwardRuntimeState::Active),
        Err(err) => Err(err),
    }
}

fn inspect_udp_port(port: u16) -> io::Result<ForwardRuntimeState> {
    match UdpSocket::bind(("127.0.0.1", port)) {
        Ok(socket) => {
            drop(socket);
            Ok(ForwardRuntimeState::Inactive(
                PortInactiveReason::PortNotBound,
            ))
        }
        Err(err) if err.kind() == io::ErrorKind::AddrInUse => Ok(ForwardRuntimeState::Active),
        Err(err) => Err(err),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        BaseImageSource, BootstrapConfig, BootstrapMode, BrokerConfig,
        DEFAULT_BOOTSTRAP_HANDSHAKE_WAIT_SECS, LifecycleConfig, MemorySpec, PortForward,
        PortProtocol, ProjectConfig, VmBootstrapConfig, VmDefinition, Workflows,
    };
    use std::collections::HashMap;
    use std::net::TcpListener;
    use std::path::PathBuf;
    use tempfile::tempdir;

    fn sample_project(state_root: &std::path::Path) -> ProjectConfig {
        let project_root = state_root.to_path_buf();
        let bootstrap_dir = project_root.join("bootstrap").join("devbox");
        let vm = VmDefinition {
            name: "devbox".to_string(),
            role_name: "devbox".to_string(),
            replica_index: 0,
            description: None,
            base_image: BaseImageSource::from_explicit(PathBuf::from("base.qcow2")),
            overlay: state_root.join("overlays/devbox.qcow2"),
            cpus: 2,
            memory: MemorySpec::new("2048 MiB", Some(2048 * 1024 * 1024)),
            port_forwards: vec![PortForward {
                host: 2222,
                guest: 22,
                protocol: PortProtocol::Tcp,
            }],
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

        ProjectConfig {
            file_path: state_root.join("castra.toml"),
            project_root,
            version: "0.1.0".to_string(),
            project_name: "demo".to_string(),
            vms: vec![vm],
            state_root: state_root.to_path_buf(),
            workflows: Workflows { init: Vec::new() },
            broker: BrokerConfig { port: 7070 },
            lifecycle: LifecycleConfig::default(),
            bootstrap: BootstrapConfig::default(),
            warnings: Vec::new(),
        }
    }

    #[test]
    fn declared_view_keeps_declared_status() {
        let temp = tempdir().expect("temp dir");
        let project = sample_project(temp.path());

        let (outcome, diagnostics) = summarize(&project, PortsView::Declared);
        assert!(diagnostics.is_empty());
        assert_eq!(outcome.view, PortsView::Declared);
        assert_eq!(outcome.declared.len(), 1);
        assert!(matches!(
            outcome.declared[0].status,
            PortForwardStatus::Declared
        ));
        assert!(outcome.declared[0].inactive_reason.is_none());
    }

    #[test]
    fn active_view_reflects_runtime_activity() {
        let temp = tempdir().expect("temp dir");
        let project = sample_project(temp.path());

        let (outcome, diagnostics) = summarize(&project, PortsView::Active);
        assert!(diagnostics.is_empty());
        assert_eq!(outcome.view, PortsView::Active);
        assert_eq!(outcome.declared.len(), 1);
        assert_eq!(outcome.declared[0].status, PortForwardStatus::Declared);
        assert_eq!(
            outcome.declared[0].inactive_reason,
            Some(PortInactiveReason::VmStopped)
        );

        let pidfile = temp.path().join("devbox.pid");
        std::fs::write(&pidfile, format!("{}\n", std::process::id())).expect("write pidfile");

        let (outcome, diagnostics) = summarize(&project, PortsView::Active);
        assert!(diagnostics.is_empty());
        assert_eq!(outcome.view, PortsView::Active);
        assert_eq!(outcome.declared.len(), 1);
        assert_eq!(outcome.declared[0].status, PortForwardStatus::Declared);
        assert_eq!(
            outcome.declared[0].inactive_reason,
            Some(PortInactiveReason::PortNotBound)
        );

        let listener = TcpListener::bind("127.0.0.1:2222").expect("bind listener");

        let (outcome, diagnostics) = summarize(&project, PortsView::Active);
        assert!(diagnostics.is_empty());
        assert_eq!(outcome.view, PortsView::Active);
        assert_eq!(outcome.declared.len(), 1);
        assert_eq!(outcome.declared[0].status, PortForwardStatus::Active);
        assert!(outcome.declared[0].inactive_reason.is_none());

        drop(listener);
    }
}
