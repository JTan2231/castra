use std::fmt::Write as _;
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
    let output = render_status_table(project, rows, broker_state, io::stdout().is_terminal());
    print!("{output}");
}

fn render_status_table(
    project: &ProjectConfig,
    rows: &[VmStatusRow],
    broker_state: BrokerProcessState,
    use_color: bool,
) -> String {
    let mut out = String::new();
    writeln!(
        out,
        "Project: {} ({})",
        project.project_name,
        project.file_path.display()
    )
    .unwrap();
    writeln!(out, "Config version: {}", project.version).unwrap();
    writeln!(out, "Broker endpoint: 127.0.0.1:{}", project.broker.port).unwrap();
    match broker_state {
        BrokerProcessState::Running { pid } => {
            writeln!(out, "Broker process: listening (pid {pid}).").unwrap();
        }
        BrokerProcessState::Offline => {
            writeln!(out, "Broker process: offline (run `castra up`).").unwrap();
        }
    }
    out.push('\n');

    if rows.is_empty() {
        writeln!(out, "No VMs defined in configuration.").unwrap();
        return out;
    }

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

    for (idx, row) in rows.iter().enumerate() {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        BaseImageSource, BrokerConfig, DEFAULT_BROKER_PORT, ManagedDiskKind, ManagedImageReference,
        MemorySpec, PortForward, PortProtocol, ProjectConfig, VmDefinition, Workflows,
    };
    use std::path::Path;
    use tempfile::tempdir;

    fn build_project(state_root: &Path) -> ProjectConfig {
        ProjectConfig {
            file_path: state_root.join("castra.toml"),
            version: "0.1.0".into(),
            project_name: "demo".into(),
            vms: vec![VmDefinition {
                name: "vm1".into(),
                description: None,
                base_image: BaseImageSource::Managed(ManagedImageReference {
                    name: "alpine-minimal".into(),
                    version: "v1".into(),
                    disk: ManagedDiskKind::RootDisk,
                }),
                overlay: state_root.join("vm1-overlay.qcow2"),
                cpus: 2,
                memory: MemorySpec::new("2048 MiB", Some(2048 * 1024 * 1024)),
                port_forwards: vec![
                    PortForward {
                        host: 2222,
                        guest: 22,
                        protocol: PortProtocol::Tcp,
                    },
                    PortForward {
                        host: 8080,
                        guest: 80,
                        protocol: PortProtocol::Tcp,
                    },
                ],
            }],
            state_root: state_root.to_path_buf(),
            workflows: Workflows { init: vec![] },
            broker: BrokerConfig {
                port: DEFAULT_BROKER_PORT,
            },
            warnings: vec![],
        }
    }

    #[test]
    fn collect_vm_status_reports_stopped_vms() {
        let dir = tempdir().unwrap();
        let project = build_project(dir.path());
        let (rows, broker_state, warnings) = collect_vm_status(&project);
        assert!(warnings.is_empty());
        assert_eq!(rows.len(), 1);
        let row = &rows[0];
        assert_eq!(row.name, "vm1");
        assert_eq!(row.state, "stopped");
        assert_eq!(row.memory, "2048MiB");
        assert_eq!(row.forwards, "2222->22/tcp, 8080->80/tcp");
        assert!(matches!(broker_state, BrokerProcessState::Offline));
    }

    #[test]
    fn format_port_forwards_outputs_dash_when_empty() {
        assert_eq!(format_port_forwards(&[]), "—");
    }

    #[test]
    fn format_port_forwards_lists_entries() {
        let forwards = vec![
            PortForward {
                host: 1000,
                guest: 10,
                protocol: PortProtocol::Tcp,
            },
            PortForward {
                host: 2000,
                guest: 20,
                protocol: PortProtocol::Udp,
            },
        ];
        let formatted = format_port_forwards(&forwards);
        assert!(formatted.contains("1000->10/tcp"));
        assert!(formatted.contains("2000->20/udp"));
    }

    #[test]
    fn format_uptime_formats_duration() {
        let uptime = Some(Duration::from_secs(3723));
        assert_eq!(format_uptime(uptime), "01:02:03");
        assert_eq!(format_uptime(None), "—");
    }

    #[test]
    fn style_state_applies_color_when_enabled() {
        let styled = style_state("running", 7, true);
        assert!(styled.contains("\u{1b}[32m"));
        let plain = style_state("unknown", 7, true);
        assert_eq!(plain, "unknown");
    }

    #[test]
    fn style_broker_applies_color_when_enabled() {
        let styled = style_broker("waiting", 7, true);
        assert!(styled.contains("\u{1b}[33m"));
        let plain = style_broker("other", 7, true);
        assert!(!plain.contains("\u{1b}"));
        assert_eq!(plain.trim_end(), "other");
    }

    #[test]
    fn print_status_table_emits_expected_rows() {
        let dir = tempdir().unwrap();
        let project = build_project(dir.path());
        let (rows, broker_state, _) = collect_vm_status(&project);
        let output = super::render_status_table(&project, &rows, broker_state, false);
        assert!(output.contains("Project: demo"));
        assert!(output.contains("CPU/MEM"));
        assert!(output.contains("vm1"));
        assert!(output.contains("2222->22/tcp"));
    }
}
