use std::time::Duration;

use crate::config::{PortForward, PortProtocol, ProjectConfig};

use super::diagnostics::{Diagnostic, Severity};
use super::outcome::VmStatusRow;
use super::project::config_state_root;
use super::runtime::{inspect_broker_state, inspect_vm_state};

pub fn collect_status(
    project: &ProjectConfig,
) -> (
    Vec<VmStatusRow>,
    super::runtime::BrokerProcessState,
    Vec<Diagnostic>,
) {
    let mut rows = Vec::with_capacity(project.vms.len());
    let mut diagnostics = Vec::new();
    let state_root = config_state_root(project);
    let broker_pidfile = broker_pid_path_from_root(&state_root);

    let (broker_state, mut broker_warnings) = inspect_broker_state(&broker_pidfile);
    diagnostics.extend(
        broker_warnings
            .drain(..)
            .map(|warning| Diagnostic::new(Severity::Warning, warning)),
    );

    for vm in &project.vms {
        let pidfile = state_root.join(format!("{}.pid", vm.name));
        let (state, uptime, mut state_warnings) = inspect_vm_state(&pidfile, &vm.name);
        diagnostics.extend(
            state_warnings
                .drain(..)
                .map(|warning| Diagnostic::new(Severity::Warning, warning)),
        );

        rows.push(VmStatusRow {
            name: vm.name.clone(),
            state,
            cpus: vm.cpus,
            memory: vm.memory.original().replace(' ', ""),
            uptime,
            broker: match broker_state {
                super::runtime::BrokerProcessState::Running { .. } => "waiting".to_string(),
                super::runtime::BrokerProcessState::Offline => "offline".to_string(),
            },
            forwards: format_port_forwards(&vm.port_forwards),
        });
    }

    (rows, broker_state, diagnostics)
}

pub fn format_port_forwards(forwards: &[PortForward]) -> String {
    let mut parts = Vec::with_capacity(forwards.len());
    for forward in forwards {
        parts.push(format!(
            "{}->{}{}",
            forward.host,
            forward.guest,
            format_protocol(forward.protocol)
        ));
    }
    if parts.is_empty() {
        "—".to_string()
    } else {
        parts.join(", ")
    }
}

fn format_protocol(protocol: PortProtocol) -> String {
    match protocol {
        PortProtocol::Tcp => "/tcp".to_string(),
        PortProtocol::Udp => "/udp".to_string(),
    }
}

pub fn format_uptime(uptime: Option<Duration>) -> String {
    match uptime {
        Some(duration) => {
            let seconds = duration.as_secs();
            let hours = seconds / 3600;
            let minutes = (seconds % 3600) / 60;
            let seconds = seconds % 60;
            format!("{hours:02}:{minutes:02}:{seconds:02}")
        }
        None => "—".to_string(),
    }
}

fn broker_pid_path_from_root(state_root: &std::path::Path) -> std::path::PathBuf {
    state_root.join("broker.pid")
}
