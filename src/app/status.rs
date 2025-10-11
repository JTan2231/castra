use std::io::{self, IsTerminal};
use std::path::PathBuf;
use std::time::Duration;

use crate::cli::StatusArgs;
use crate::config::{PortForward, PortProtocol, ProjectConfig};
use crate::error::CliResult;

use super::display::colorize;
use super::project::{emit_config_warnings, load_or_default_project};
use super::runtime::{BrokerProcessState, inspect_broker_state, inspect_vm_state};

pub fn handle_status(args: StatusArgs, config_override: Option<&PathBuf>) -> CliResult<()> {
    let project = load_or_default_project(config_override, args.skip_discovery)?;

    emit_config_warnings(&project.warnings);

    let (status_rows, broker_state, status_warnings) = collect_vm_status(&project);
    for warning in status_warnings {
        eprintln!("Warning: {warning}");
    }

    print_status_table(&project, &status_rows, broker_state);
    Ok(())
}

pub struct VmStatusRow {
    pub name: String,
    pub state: String,
    pub cpus: u32,
    pub memory: String,
    pub uptime: Option<Duration>,
    pub broker: String,
    pub forwards: String,
}

pub fn collect_vm_status(
    project: &ProjectConfig,
) -> (Vec<VmStatusRow>, BrokerProcessState, Vec<String>) {
    let mut rows = Vec::with_capacity(project.vms.len());
    let mut warnings = Vec::new();
    let state_root = super::project::config_state_root(project);
    let broker_pidfile = state_root.join("broker.pid");

    let (broker_state, mut broker_warnings) = inspect_broker_state(&broker_pidfile);
    warnings.append(&mut broker_warnings);

    for vm in &project.vms {
        let pidfile = state_root.join(format!("{}.pid", vm.name));
        let (state, uptime, mut state_warnings) = inspect_vm_state(&pidfile, &vm.name);
        warnings.append(&mut state_warnings);

        rows.push(VmStatusRow {
            name: vm.name.clone(),
            state,
            cpus: vm.cpus,
            memory: vm.memory.original().replace(' ', ""),
            uptime,
            broker: match broker_state {
                BrokerProcessState::Running { .. } => "waiting".to_string(),
                BrokerProcessState::Offline => "offline".to_string(),
            },
            forwards: format_port_forwards(&vm.port_forwards),
        });
    }

    (rows, broker_state, warnings)
}

pub fn print_status_table(
    project: &ProjectConfig,
    rows: &[VmStatusRow],
    broker_state: BrokerProcessState,
) {
    println!(
        "Project: {} ({})",
        project.project_name,
        project.file_path.display()
    );
    println!("Config version: {}", project.version);
    println!("Broker endpoint: 127.0.0.1:{}", project.broker.port);
    match broker_state {
        BrokerProcessState::Running { pid } => println!("Broker process: listening (pid {pid})."),
        BrokerProcessState::Offline => println!("Broker process: offline (run `castra up`)."),
    }
    println!();

    if rows.is_empty() {
        println!("No VMs defined in configuration.");
        return;
    }

    let use_color = io::stdout().is_terminal();
    let cpu_mem: Vec<String> = rows
        .iter()
        .map(|row| format!("{}/{}", row.cpus, row.memory))
        .collect();

    let vm_width = rows
        .iter()
        .map(|row| row.name.len())
        .max()
        .unwrap_or(2)
        .max("VM".len());
    let state_width = rows
        .iter()
        .map(|row| row.state.len())
        .max()
        .unwrap_or(5)
        .max("STATE".len());
    let cpu_mem_width = cpu_mem
        .iter()
        .map(|value| value.len())
        .max()
        .unwrap_or(1)
        .max("CPU/MEM".len());
    let uptime_width = rows
        .iter()
        .map(|row| format_uptime(row.uptime).len())
        .max()
        .unwrap_or(1)
        .max("UPTIME".len());
    let broker_width = rows
        .iter()
        .map(|row| row.broker.len())
        .max()
        .unwrap_or(1)
        .max("BROKER".len());

    println!(
        "{:<vm_width$}  {:<state_width$}  {:>cpu_mem_width$}  {:>uptime_width$}  {:<broker_width$}  {}",
        "VM",
        "STATE",
        "CPU/MEM",
        "UPTIME",
        "BROKER",
        "FORWARDS",
        vm_width = vm_width,
        state_width = state_width,
        cpu_mem_width = cpu_mem_width,
        uptime_width = uptime_width,
        broker_width = broker_width,
    );

    for (idx, row) in rows.iter().enumerate() {
        let state = style_state(&row.state, state_width, use_color);
        let broker = style_broker(&row.broker, broker_width, use_color);
        println!(
            "{:<vm_width$}  {}  {:>cpu_mem_width$}  {:>uptime_width$}  {}  {}",
            row.name,
            state,
            cpu_mem[idx],
            format_uptime(row.uptime),
            broker,
            row.forwards,
            vm_width = vm_width,
            cpu_mem_width = cpu_mem_width,
            uptime_width = uptime_width,
        );
    }

    println!();
    println!(
        "Legend: BROKER reachable = host broker handshake OK; waiting = broker up, guest not connected; offline = listener not running."
    );
    println!("States: stopped | starting | running | shutting_down | error");
    println!("Exit codes: 0 on success; non-zero if any VM in error.");
}

fn format_port_forwards(forwards: &[PortForward]) -> String {
    if forwards.is_empty() {
        return "—".to_string();
    }

    let mut entries = Vec::with_capacity(forwards.len());
    for forward in forwards {
        entries.push(format!(
            "{}->{}{}",
            forward.host,
            forward.guest,
            match forward.protocol {
                PortProtocol::Tcp => "/tcp",
                PortProtocol::Udp => "/udp",
            }
        ));
    }
    entries.join(", ")
}

fn format_uptime(value: Option<Duration>) -> String {
    if let Some(duration) = value {
        let total_seconds = duration.as_secs();
        let hours = total_seconds / 3600;
        let minutes = (total_seconds % 3600) / 60;
        let seconds = total_seconds % 60;
        return format!("{hours:02}:{minutes:02}:{seconds:02}");
    }
    "—".to_string()
}

fn style_state(state: &str, width: usize, colored: bool) -> String {
    let padded = format!("{:<width$}", state, width = width);
    let code = match state {
        "running" => "32",
        "starting" => "33",
        "shutting_down" => "33",
        "error" => "31",
        "stopped" => "90",
        _ => return padded,
    };
    colorize(&padded, code, colored)
}

fn style_broker(status: &str, width: usize, colored: bool) -> String {
    let padded = format!("{:<width$}", status, width = width);
    let code = match status {
        "reachable" => "32",
        "waiting" => "33",
        "offline" => "90",
        "—" => "90",
        _ => return padded,
    };
    colorize(&padded, code, colored)
}
