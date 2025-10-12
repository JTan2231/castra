use std::collections::HashSet;

use crate::config::ProjectConfig;

use super::diagnostics::{Diagnostic, Severity};
use super::outcome::{
    PortConflictRow, PortForwardRow, PortForwardStatus, PortsOutcome, VmPortDetail,
};

pub fn summarize(project: &ProjectConfig) -> (PortsOutcome, Vec<Diagnostic>) {
    let (conflicts, broker_collision) = project.port_conflicts();
    let conflict_ports: HashSet<u16> = conflicts.iter().map(|c| c.port).collect();
    let broker_conflict_port = broker_collision.as_ref().map(|c| c.port);

    let mut declared = Vec::new();
    for vm in &project.vms {
        for forward in &vm.port_forwards {
            let status = if conflict_ports.contains(&forward.host) {
                PortForwardStatus::Conflicting
            } else if broker_conflict_port == Some(forward.host) {
                PortForwardStatus::BrokerReserved
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

    let mut diagnostics = Vec::new();
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
