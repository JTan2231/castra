use std::fmt::Write as _;
use std::io::{self, IsTerminal};
use std::path::PathBuf;

use crate::Result;
use crate::cli::StatusArgs;
use crate::core::operations;
use crate::core::options::StatusOptions;
use crate::core::outcome::{BrokerState, StatusOutcome};
use crate::core::project::format_config_warnings;
use crate::core::status::format_uptime;

use super::common::{config_load_options, emit_diagnostics, split_config_warnings};

pub fn handle_status(args: StatusArgs, config_override: Option<&PathBuf>) -> Result<()> {
    let options = StatusOptions {
        config: config_load_options(config_override, args.skip_discovery),
    };

    let output = operations::status(options, None)?;

    let (config_warnings, other) = split_config_warnings(&output.diagnostics);
    if let Some(message) = format_config_warnings(&config_warnings) {
        eprint!("{message}");
    }
    emit_diagnostics(&other);

    let use_color = io::stdout().is_terminal();
    let table = render_status_table(&output.value, use_color);
    print!("{table}");

    Ok(())
}

fn render_status_table(outcome: &StatusOutcome, use_color: bool) -> String {
    let mut out = String::new();
    writeln!(
        out,
        "Project: {} ({})",
        outcome.project_name,
        outcome.project_path.display()
    )
    .unwrap();
    writeln!(out, "Config version: {}", outcome.config_version).unwrap();
    writeln!(out, "Broker endpoint: 127.0.0.1:{}", outcome.broker_port).unwrap();
    match outcome.broker_state {
        BrokerState::Running { pid } => {
            writeln!(out, "Broker process: listening (pid {pid}).").unwrap();
        }
        BrokerState::Offline => {
            writeln!(out, "Broker process: offline (run `castra up`).").unwrap();
        }
    }
    out.push('\n');

    if outcome.rows.is_empty() {
        writeln!(out, "No VMs defined in configuration.").unwrap();
        return out;
    }

    let cpu_mem: Vec<String> = outcome
        .rows
        .iter()
        .map(|row| format!("{}/{}", row.cpus, row.memory))
        .collect();

    let vm_width = outcome
        .rows
        .iter()
        .map(|row| row.name.len())
        .max()
        .unwrap_or(2)
        .max("VM".len());
    let state_width = outcome
        .rows
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
    let uptime_width = outcome
        .rows
        .iter()
        .map(|row| format_uptime(row.uptime).len())
        .max()
        .unwrap_or(1)
        .max("UPTIME".len());
    let broker_width = outcome
        .rows
        .iter()
        .map(|row| row.broker.len())
        .max()
        .unwrap_or(1)
        .max("BROKER".len());

    writeln!(
        out,
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
    )
    .unwrap();

    for (idx, row) in outcome.rows.iter().enumerate() {
        let state = style_state(&row.state, state_width, use_color);
        let broker = style_broker(&row.broker, broker_width, use_color);
        writeln!(
            out,
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
        )
        .unwrap();
    }

    out.push('\n');
    writeln!(
        out,
        "Legend: BROKER reachable = host broker handshake OK; waiting = broker up, guest not connected; offline = listener not running."
    )
    .unwrap();
    writeln!(
        out,
        "States: stopped | starting | running | shutting_down | error"
    )
    .unwrap();
    writeln!(
        out,
        "Exit codes: 0 on success; non-zero if any VM in error."
    )
    .unwrap();

    out
}

fn style_state(state: &str, width: usize, use_color: bool) -> String {
    if !use_color {
        return format!("{:width$}", state, width = width);
    }

    let code = match state {
        "running" => "32",
        "starting" | "shutting_down" => "33",
        "error" => "31",
        _ => "37",
    };
    colorize(state, code, width)
}

fn style_broker(state: &str, width: usize, use_color: bool) -> String {
    if !use_color {
        return format!("{:width$}", state, width = width);
    }

    let code = match state {
        "waiting" => "33",
        "reachable" => "32",
        "offline" => "31",
        _ => "37",
    };
    colorize(state, code, width)
}

fn colorize(text: &str, code: &str, width: usize) -> String {
    format!("\u{001b}[{code}m{:width$}\u{001b}[0m", text, width = width)
}
