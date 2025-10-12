use std::cmp;
use std::path::PathBuf;

use crate::Result;
use crate::cli::PortsArgs;
use crate::core::operations;
use crate::core::options::PortsOptions;
use crate::core::outcome::{PortForwardStatus, PortsOutcome};
use crate::core::project::format_config_warnings;

use super::common::{config_load_options, emit_diagnostics, split_config_warnings};

pub fn handle_ports(args: PortsArgs, config_override: Option<&PathBuf>) -> Result<()> {
    let options = PortsOptions {
        config: config_load_options(config_override, false),
        verbose: args.verbose,
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
    println!(
        "Project: {} ({})",
        outcome.project_name,
        outcome.project_path.display()
    );
    println!("Config version: {}", outcome.config_version);
    println!("Broker endpoint: 127.0.0.1:{}", outcome.broker_port);
    println!("(start the broker via `castra up` once available)");
    println!();

    if outcome.declared.is_empty() {
        println!(
            "No port forwards declared in {}.",
            outcome.project_path.display()
        );
    } else {
        let vm_width = cmp::max(
            "VM".len(),
            outcome
                .declared
                .iter()
                .map(|row| row.vm.len())
                .max()
                .unwrap_or(0),
        );
        println!("Declared forwards:");
        println!(
            "  {vm:<width$}  {:>5}  {:>5}  {:<5}  {}",
            "HOST",
            "GUEST",
            "PROTO",
            "STATUS",
            vm = "VM",
            width = vm_width
        );
        for row in &outcome.declared {
            println!(
                "  {vm:<width$}  {:>5}  {:>5}  {:<5}  {status}",
                row.forward.host,
                row.forward.guest,
                row.forward.protocol,
                status = status_label(row.status),
                vm = row.vm,
                width = vm_width
            );
        }
    }

    if !outcome.without_forwards.is_empty() {
        println!();
        println!(
            "VMs without host forwards: {}",
            outcome.without_forwards.join(", ")
        );
    }

    if verbose {
        println!();
        println!("VM details:");
        for vm in &outcome.vm_details {
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
        if !outcome.vm_details.is_empty() {
            println!();
        }
    }

    if !outcome.conflicts.is_empty() {
        println!();
        for conflict in &outcome.conflicts {
            println!(
                "Warning: host port {} is declared by multiple VMs: {}.",
                conflict.port,
                conflict.vm_names.join(", ")
            );
        }
    }
}

fn status_label(status: PortForwardStatus) -> &'static str {
    match status {
        PortForwardStatus::Declared => "declared",
        PortForwardStatus::Active => "active",
        PortForwardStatus::Conflicting => "conflict",
        PortForwardStatus::BrokerReserved => "broker-reserved",
    }
}
