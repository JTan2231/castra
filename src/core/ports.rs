use std::collections::HashSet;

use crate::config::ProjectConfig;

use super::diagnostics::{Diagnostic, Severity};
use super::options::PortsView;
use super::outcome::{
    PortConflictRow, PortForwardRow, PortForwardStatus, PortsOutcome, VmPortDetail,
};
use super::project::config_state_root;
use super::runtime::inspect_vm_state;

pub fn summarize(project: &ProjectConfig, view: PortsView) -> (PortsOutcome, Vec<Diagnostic>) {
    let mut diagnostics = Vec::new();
    let (conflicts, broker_collision) = project.port_conflicts();
    let conflict_ports: HashSet<u16> = conflicts.iter().map(|c| c.port).collect();
    let broker_conflict_port = broker_collision.as_ref().map(|c| c.port);

    let runtime_active: Option<HashSet<String>> = if matches!(view, PortsView::Active) {
        let state_root = config_state_root(project);
        let mut active = HashSet::new();
        for vm in &project.vms {
            let pidfile = state_root.join(format!("{}.pid", vm.name));
            let (state, _uptime, warnings) = inspect_vm_state(&pidfile, &vm.name);
            diagnostics.extend(
                warnings
                    .into_iter()
                    .map(|warning| Diagnostic::new(Severity::Warning, warning)),
            );
            if state == "running" {
                active.insert(vm.name.clone());
            }
        }
        Some(active)
    } else {
        None
    };

    let mut declared = Vec::new();
    for vm in &project.vms {
        for forward in &vm.port_forwards {
            let status = if conflict_ports.contains(&forward.host) {
                PortForwardStatus::Conflicting
            } else if broker_conflict_port == Some(forward.host) {
                PortForwardStatus::BrokerReserved
            } else if runtime_active
                .as_ref()
                .map_or(false, |set| set.contains(&vm.name))
            {
                PortForwardStatus::Active
            } else {
                PortForwardStatus::Declared
            };

            declared.push(PortForwardRow {
                vm: vm.name.clone(),
                forward: forward.clone(),
                status,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        BaseImageSource, BrokerConfig, MemorySpec, PortForward, PortProtocol, ProjectConfig,
        VmDefinition, Workflows,
    };
    use tempfile::tempdir;

    fn sample_project(state_root: &std::path::Path) -> ProjectConfig {
        let vm = VmDefinition {
            name: "devbox".to_string(),
            role_name: "devbox".to_string(),
            replica_index: 0,
            description: None,
            base_image: BaseImageSource::Path("base.qcow2".into()),
            overlay: state_root.join("overlays/devbox.qcow2"),
            cpus: 2,
            memory: MemorySpec::new("2048 MiB", Some(2048 * 1024 * 1024)),
            port_forwards: vec![PortForward {
                host: 2222,
                guest: 22,
                protocol: PortProtocol::Tcp,
            }],
        };

        ProjectConfig {
            file_path: state_root.join("castra.toml"),
            version: "0.1.0".to_string(),
            project_name: "demo".to_string(),
            vms: vec![vm],
            state_root: state_root.to_path_buf(),
            workflows: Workflows { init: Vec::new() },
            broker: BrokerConfig { port: 7070 },
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
    }

    #[test]
    fn active_view_marks_running_vm_forwards() {
        let temp = tempdir().expect("temp dir");
        let project = sample_project(temp.path());

        let (outcome, diagnostics) = summarize(&project, PortsView::Active);
        assert!(diagnostics.is_empty());
        assert_eq!(outcome.view, PortsView::Active);
        assert_eq!(outcome.declared.len(), 1);
        assert!(matches!(
            outcome.declared[0].status,
            PortForwardStatus::Declared
        ));

        let pidfile = temp.path().join("devbox.pid");
        std::fs::write(&pidfile, format!("{}\n", std::process::id())).expect("write pidfile");

        let (outcome, diagnostics) = summarize(&project, PortsView::Active);
        assert!(diagnostics.is_empty());
        assert_eq!(outcome.view, PortsView::Active);
        assert_eq!(outcome.declared.len(), 1);
        assert!(matches!(
            outcome.declared[0].status,
            PortForwardStatus::Active
        ));
    }
}
