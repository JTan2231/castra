use std::cmp;
use std::path::PathBuf;

use crate::Result;
use crate::cli::PortsArgs;
use crate::core::operations;
use crate::core::options::{PortsOptions, PortsView};
use crate::core::outcome::{
    PortForwardStatus, PortInactiveReason, PortsOutcome, ProjectPortsOutcome,
};
use crate::core::project::format_config_warnings;

use super::common::{config_load_options, emit_diagnostics, split_config_warnings};

pub fn handle_ports(args: PortsArgs, config_override: Option<&PathBuf>) -> Result<()> {
    let options = PortsOptions {
        config: config_load_options(config_override, args.skip_discovery, "ports")?,
        verbose: args.verbose,
        view: if args.active {
            PortsView::Active
        } else {
            PortsView::Declared
        },
        workspace: args.workspace.clone(),
    };

    let output = operations::ports(options, None)?;

    let (config_warnings, other) = split_config_warnings(&output.diagnostics);
    if let Some(message) = format_config_warnings(&config_warnings) {
        eprint!("{message}");
    }
    emit_diagnostics(&other);

    render_ports(&output.value, args.verbose);

    Ok(())
}

fn render_ports(outcome: &PortsOutcome, verbose: bool) {
    if outcome.projects.is_empty() {
        println!("No active workspaces detected.");
        return;
    }

    let multi = outcome.aggregated || outcome.projects.len() > 1;
    for (idx, project) in outcome.projects.iter().enumerate() {
        if idx > 0 {
            println!();
        }
        if multi {
            let mut header = project.project_name.clone();
            if let Some(id) = &project.workspace_id {
                header.push_str(&format!(" ({id})"));
            }
            println!("=== {header} ===");
        }
        render_project_ports(project, verbose, outcome.view);
    }
}

fn render_project_ports(project: &ProjectPortsOutcome, verbose: bool, view: PortsView) {
    println!(
        "Project: {} ({})",
        project.project_name,
        project.project_path.display()
    );
    if let Some(config_path) = &project.config_path {
        println!("Config path: {}", config_path.display());
    }
    println!("Config version: {}", project.config_version);
    if let Some(id) = &project.workspace_id {
        println!("Workspace ID: {id}");
    }
    if let Some(state_root) = &project.state_root {
        println!("State root: {}", state_root.display());
    }
    println!("Broker endpoint: 127.0.0.1:{}", project.broker_port);
    println!("(start the broker via `castra up` once available)");
    if matches!(view, PortsView::Active) {
        println!("STATUS column reflects runtime state; stopped VMs show as inactive.");
    }
    println!();

    if project.declared.is_empty() {
        println!(
            "No port forwards declared in {}.",
            project.project_path.display()
        );
    } else {
        let vm_width = cmp::max(
            "VM".len(),
            project
                .declared
                .iter()
                .map(|row| row.vm.len())
                .max()
                .unwrap_or(0),
        );
        let heading = match view {
            PortsView::Declared => "Declared forwards:",
            PortsView::Active => "Runtime forwards:",
        };
        println!("{heading}");
        println!(
            "  {vm:<width$}  {:>5}  {:>5}  {:<5}  {}",
            "HOST",
            "GUEST",
            "PROTO",
            "STATUS",
            vm = "VM",
            width = vm_width
        );
        for row in &project.declared {
            println!(
                "  {vm:<width$}  {:>5}  {:>5}  {:<5}  {status}",
                row.forward.host,
                row.forward.guest,
                row.forward.protocol,
                status = status_label(row.status, view, row.inactive_reason),
                vm = row.vm,
                width = vm_width
            );
        }
    }

    if !project.without_forwards.is_empty() {
        println!();
        println!(
            "VMs without host forwards: {}",
            project.without_forwards.join(", ")
        );
    }

    if verbose {
        println!();
        println!("VM details:");
        for vm in &project.vm_details {
            println!("  {}", vm.name);
            if let Some(desc) = &vm.description {
                println!("    description: {desc}");
            }
            println!("    base_image: {}", vm.base_image);
            println!("    overlay: {}", vm.overlay.display());
            println!("    cpus: {}", vm.cpus);
            println!("    memory: {}", vm.memory);
            if let Some(bytes) = vm.memory_bytes {
                println!("    memory_bytes: {}", bytes);
            }
            if vm.port_forwards.is_empty() {
                println!("    port_forwards: (none)");
            }
        }
        if !project.vm_details.is_empty() {
            println!();
        }
    }

    if !project.conflicts.is_empty() {
        println!();
        for conflict in &project.conflicts {
            println!(
                "Warning: host port {} is declared by multiple VMs: {}.",
                conflict.port,
                conflict.vm_names.join(", ")
            );
        }
    }
}

fn status_label(
    status: PortForwardStatus,
    view: PortsView,
    reason: Option<PortInactiveReason>,
) -> String {
    match status {
        PortForwardStatus::Declared => match view {
            PortsView::Declared => "declared".to_string(),
            PortsView::Active => match reason {
                Some(PortInactiveReason::VmStopped) => "inactive (vm stopped)".to_string(),
                Some(PortInactiveReason::PortNotBound) => "inactive (port not bound)".to_string(),
                Some(PortInactiveReason::InspectionUnavailable) => {
                    "inactive (inspection unavailable)".to_string()
                }
                None => "inactive".to_string(),
            },
        },
        PortForwardStatus::Active => "active".to_string(),
        PortForwardStatus::Conflicting => "conflict".to_string(),
        PortForwardStatus::BrokerReserved => "broker-reserved".to_string(),
    }
}
