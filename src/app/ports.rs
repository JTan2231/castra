use std::cmp;
use std::collections::HashSet;
use std::path::PathBuf;

use crate::cli::PortsArgs;
use crate::config::ProjectConfig;
use crate::error::CliResult;

use super::project::{emit_config_warnings, load_or_default_project};

pub fn handle_ports(args: PortsArgs, config_override: Option<&PathBuf>) -> CliResult<()> {
    let project = load_or_default_project(config_override, false)?;

    emit_config_warnings(&project.warnings);

    print_port_overview(&project, args.verbose);
    Ok(())
}

pub fn print_port_overview(project: &ProjectConfig, verbose: bool) {
    println!(
        "Project: {} ({})",
        project.project_name,
        project.file_path.display()
    );
    println!("Config version: {}", project.version);
    println!("Broker endpoint: 127.0.0.1:{}", project.broker.port);
    println!("(start the broker via `castra up` once available)");
    println!();

    let (conflicts, broker_collision) = project.port_conflicts();
    let conflict_ports: HashSet<u16> = conflicts.iter().map(|c| c.port).collect();
    let broker_conflict_port = broker_collision.as_ref().map(|c| c.port);

    let mut rows = Vec::new();
    for vm in &project.vms {
        for forward in &vm.port_forwards {
            rows.push((
                vm.name.as_str(),
                forward.host,
                forward.guest,
                forward.protocol,
            ));
        }
    }

    let vm_width = cmp::max(
        "VM".len(),
        project
            .vms
            .iter()
            .map(|vm| vm.name.len())
            .max()
            .unwrap_or(0),
    );

    if rows.is_empty() {
        println!(
            "No port forwards declared in {}.",
            project.file_path.display()
        );
    } else {
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

        for (vm_name, host, guest, protocol) in rows {
            let mut status = "declared";
            if conflict_ports.contains(&host) {
                status = "conflict";
            } else if broker_conflict_port == Some(host) {
                status = "broker-reserved";
            }

            println!(
                "  {vm:<width$}  {:>5}  {:>5}  {:<5}  {status}",
                host,
                guest,
                protocol,
                vm = vm_name,
                width = vm_width
            );
        }
    }

    let without_forwards: Vec<&str> = project
        .vms
        .iter()
        .filter(|vm| vm.port_forwards.is_empty())
        .map(|vm| vm.name.as_str())
        .collect();

    if !without_forwards.is_empty() {
        println!();
        println!("VMs without host forwards: {}", without_forwards.join(", "));
    }

    if verbose {
        println!();
        println!("VM details:");
        for vm in &project.vms {
            println!("  {}", vm.name);
            if let Some(desc) = &vm.description {
                println!("    description: {desc}");
            }
            println!("    base_image: {}", vm.base_image.describe());
            println!("    overlay: {}", vm.overlay.display());
            println!("    cpus: {}", vm.cpus);
            println!("    memory: {}", vm.memory.original());
            if let Some(bytes) = vm.memory.bytes() {
                println!("    memory_bytes: {}", bytes);
            }
            if vm.port_forwards.is_empty() {
                println!("    port_forwards: (none)");
            }
        }
        if !project.workflows.init.is_empty() {
            println!();
            println!("Init workflow steps:");
            for step in &project.workflows.init {
                println!("  - {step}");
            }
        }
    }

    if !conflicts.is_empty() {
        eprintln!();
        for conflict in &conflicts {
            eprintln!(
                "Warning: host port {} is declared by multiple VMs: {}.",
                conflict.port,
                conflict.vm_names.join(", ")
            );
        }
    }

    if let Some(collision) = broker_collision {
        eprintln!(
            "Warning: host port {} overlaps with the castra broker. Adjust the broker port or the forward.",
            collision.port
        );
    }
}
