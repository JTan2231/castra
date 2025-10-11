use std::path::PathBuf;

use crate::cli::UpArgs;
use crate::error::{CliError, CliResult};

use super::project::{emit_config_warnings, load_or_default_project};
use super::runtime::{
    check_disk_space, check_host_capacity, ensure_ports_available, ensure_vm_assets, launch_vm,
    prepare_runtime_context, start_broker,
};
use super::status::collect_vm_status;

pub fn handle_up(args: UpArgs, config_override: Option<&PathBuf>) -> CliResult<()> {
    let project = load_or_default_project(config_override, args.skip_discovery)?;

    emit_config_warnings(&project.warnings);

    let (status_rows, _, mut status_warnings) = collect_vm_status(&project);
    let running: Vec<_> = status_rows
        .iter()
        .filter(|row| row.state == "running")
        .map(|row| row.name.clone())
        .collect();
    for warning in status_warnings.drain(..) {
        eprintln!("Warning: {warning}");
    }
    if !running.is_empty() {
        return Err(CliError::PreflightFailed {
            message: format!(
                "VMs already running: {}. Use `castra status` or `castra down` before invoking `up` again.",
                running.join(", ")
            ),
        });
    }

    check_host_capacity(&project, args.force)?;
    let context = prepare_runtime_context(&project)?;
    check_disk_space(&project, &context, args.force)?;
    ensure_ports_available(&project)?;

    let mut preparations = Vec::with_capacity(project.vms.len());
    for vm in &project.vms {
        let prep = ensure_vm_assets(vm, &context)?;
        if let Some(managed) = &prep.managed {
            for event in &managed.events {
                println!(
                    "→ {} {}: {}",
                    managed.spec.id, managed.spec.version, event.message
                );
            }
        }
        if prep.overlay_created {
            println!(
                "Prepared overlay for VM `{}` at {}.",
                vm.name,
                vm.overlay.display()
            );
        }
        preparations.push(prep);
    }

    start_broker(&project, &context)?;

    for (vm, prep) in project.vms.iter().zip(preparations.iter()) {
        launch_vm(vm, &prep.assets, &context)?;
    }

    println!("Launched {} VM(s).", project.vms.len());
    println!("Use `castra status` to monitor startup progress.");
    Ok(())
}
