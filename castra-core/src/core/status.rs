use std::fs;
use std::io::{self, ErrorKind};
use std::path::Path;
use std::time::Duration;

use crate::config::{PortForward, PortProtocol, ProjectConfig};

use super::diagnostics::{Diagnostic, Severity};
use super::outcome::VmStatusRow;
use super::project::config_state_root;
use super::runtime::inspect_vm_state;

pub const HANDSHAKE_FRESHNESS: Duration = Duration::from_secs(45);

#[derive(Debug)]
pub struct StatusSnapshot {
    pub rows: Vec<VmStatusRow>,
    pub diagnostics: Vec<Diagnostic>,
    pub reachable: bool,
}

pub fn collect_status(project: &ProjectConfig) -> StatusSnapshot {
    let mut rows = Vec::with_capacity(project.vms.len());
    let mut diagnostics = Vec::new();
    let state_root = config_state_root(project);

    for vm in &project.vms {
        let pidfile = state_root.join(format!("{}.pid", vm.name));
        let (state, uptime, mut state_warnings) = inspect_vm_state(&pidfile, &vm.name);
        diagnostics.extend(
            state_warnings
                .drain(..)
                .map(|warning| Diagnostic::new(Severity::Warning, warning)),
        );

        if state != "running" {
            cleanup_orphan_overlay(&vm.name, &vm.overlay, &mut diagnostics);
        }

        rows.push(VmStatusRow {
            name: vm.name.clone(),
            state,
            cpus: vm.cpus,
            memory: vm.memory.original().replace(' ', ""),
            uptime,
            forwards: format_port_forwards(&vm.port_forwards),
        });
    }

    let reachable = rows.iter().any(|row| row.state == "running");

    StatusSnapshot {
        rows,
        diagnostics,
        reachable,
    }
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

fn cleanup_orphan_overlay(vm_name: &str, overlay_path: &Path, diagnostics: &mut Vec<Diagnostic>) {
    match remove_overlay_if_present(overlay_path) {
        Ok(Some(bytes)) => {
            diagnostics.push(
                Diagnostic::new(
                    Severity::Info,
                    format!(
                        "Removed stale ephemeral overlay {} for VM `{vm_name}` (reclaimed {}).",
                        overlay_path.display(),
                        format_bytes(bytes)
                    ),
                )
                .with_help("Guest changes are discarded on shutdown. Export via SSH before stopping if you need to retain data."),
            );
        }
        Ok(None) => {}
        Err(err) => {
            diagnostics.push(
                Diagnostic::new(
                    Severity::Warning,
                    format!(
                        "Failed to remove stale ephemeral overlay {} for VM `{vm_name}`: {err}",
                        overlay_path.display()
                    ),
                )
                .with_help("Remove it manually (e.g. `rm <path>`) or run `castra clean --include-overlays`."),
            );
        }
    }
}

fn remove_overlay_if_present(path: &Path) -> io::Result<Option<u64>> {
    match fs::metadata(path) {
        Ok(metadata) => {
            if metadata.is_file() {
                let bytes = metadata.len();
                match fs::remove_file(path) {
                    Ok(_) => Ok(Some(bytes)),
                    Err(err) if err.kind() == ErrorKind::NotFound => Ok(None),
                    Err(err) => Err(err),
                }
            } else {
                Ok(None)
            }
        }
        Err(err) if err.kind() == ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err),
    }
}

fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} {}", UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

fn format_protocol(protocol: PortProtocol) -> String {
    match protocol {
        PortProtocol::Tcp => "/tcp".to_string(),
        PortProtocol::Udp => "/udp".to_string(),
    }
}
