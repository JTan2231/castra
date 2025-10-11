use std::path::PathBuf;

use crate::cli::DownArgs;
use crate::error::CliResult;

use super::project::{config_state_root, emit_config_warnings, load_or_default_project};
use super::runtime::{shutdown_broker, shutdown_vm};

pub fn handle_down(args: DownArgs, config_override: Option<&PathBuf>) -> CliResult<()> {
    let project = load_or_default_project(config_override, args.skip_discovery)?;

    emit_config_warnings(&project.warnings);

    let state_root = config_state_root(&project);
    let mut had_running = false;

    for vm in &project.vms {
        if shutdown_vm(vm, &state_root)? {
            had_running = true;
        }
    }

    let broker_running = shutdown_broker(&state_root)?;

    match (had_running, broker_running) {
        (false, false) => println!("No running VMs or broker detected."),
        (true, false) => println!("All VMs have been stopped."),
        (false, true) => println!("Broker listener stopped."),
        (true, true) => {
            println!("All VMs have been stopped.");
            println!("Broker listener stopped.");
        }
    }

    Ok(())
}
