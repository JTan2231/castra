use std::cmp;
use std::collections::HashSet;
use std::fmt::Write as _;
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
    let (out, err) = render_port_overview(project, verbose);
    print!("{out}");
    if !err.is_empty() {
        eprint!("{err}");
    }
}

fn render_port_overview(project: &ProjectConfig, verbose: bool) -> (String, String) {
    let mut out = String::new();
    let mut err = String::new();

    writeln!(
        out,
        "Project: {} ({})",
        project.project_name,
        project.file_path.display()
    )
    .unwrap();
    writeln!(out, "Config version: {}", project.version).unwrap();
    writeln!(out, "Broker endpoint: 127.0.0.1:{}", project.broker.port).unwrap();
    writeln!(out, "(start the broker via `castra up` once available)").unwrap();
    out.push('\n');

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
        writeln!(
            out,
            "No port forwards declared in {}.",
            project.file_path.display()
        )
        .unwrap();
    } else {
        writeln!(out, "Declared forwards:").unwrap();
        writeln!(
            out,
            "  {vm:<width$}  {:>5}  {:>5}  {:<5}  {}",
            "HOST",
            "GUEST",
            "PROTO",
            "STATUS",
            vm = "VM",
            width = vm_width
        )
        .unwrap();

        for (vm_name, host, guest, protocol) in rows {
            let mut status = "declared";
            if conflict_ports.contains(&host) {
                status = "conflict";
            } else if broker_conflict_port == Some(host) {
                status = "broker-reserved";
            }

            writeln!(
                out,
                "  {vm:<width$}  {:>5}  {:>5}  {:<5}  {status}",
                host,
                guest,
                protocol,
                vm = vm_name,
                width = vm_width
            )
            .unwrap();
        }
    }

    let without_forwards: Vec<&str> = project
        .vms
        .iter()
        .filter(|vm| vm.port_forwards.is_empty())
        .map(|vm| vm.name.as_str())
        .collect();

    if !without_forwards.is_empty() {
        out.push('\n');
        writeln!(
            out,
            "VMs without host forwards: {}",
            without_forwards.join(", ")
        )
        .unwrap();
    }

    if verbose {
        out.push('\n');
        writeln!(out, "VM details:").unwrap();
        for vm in &project.vms {
            writeln!(out, "  {}", vm.name).unwrap();
            if let Some(desc) = &vm.description {
                writeln!(out, "    description: {desc}").unwrap();
            }
            writeln!(out, "    base_image: {}", vm.base_image.describe()).unwrap();
            writeln!(out, "    overlay: {}", vm.overlay.display()).unwrap();
            writeln!(out, "    cpus: {}", vm.cpus).unwrap();
            writeln!(out, "    memory: {}", vm.memory.original()).unwrap();
            if let Some(bytes) = vm.memory.bytes() {
                writeln!(out, "    memory_bytes: {}", bytes).unwrap();
            }
            if vm.port_forwards.is_empty() {
                writeln!(out, "    port_forwards: (none)").unwrap();
            }
        }
        if !project.workflows.init.is_empty() {
            out.push('\n');
            writeln!(out, "Init workflow steps:").unwrap();
            for step in &project.workflows.init {
                writeln!(out, "  - {step}").unwrap();
            }
        }
    }

    if !conflicts.is_empty() {
        err.push('\n');
        for conflict in &conflicts {
            writeln!(
                err,
                "Warning: host port {} is declared by multiple VMs: {}.",
                conflict.port,
                conflict.vm_names.join(", ")
            )
            .unwrap();
        }
    }

    if let Some(collision) = broker_collision {
        writeln!(
            err,
            "Warning: host port {} overlaps with the castra broker. Adjust the broker port or the forward.",
            collision.port
        )
        .unwrap();
    }

    (out, err)
}

#[cfg(test)]
mod tests {
    use crate::config::{
        BaseImageSource, BrokerConfig, ManagedDiskKind, ManagedImageReference, MemorySpec,
        PortForward, PortProtocol, ProjectConfig, VmDefinition, Workflows,
    };
    use std::path::Path;
    use tempfile::tempdir;

    fn project_with_forwards(
        root: &Path,
        forwards_per_vm: Vec<(&str, Vec<PortForward>)>,
        broker_port: u16,
    ) -> ProjectConfig {
        let mut vms = Vec::new();
        for (name, forwards) in forwards_per_vm {
            vms.push(VmDefinition {
                name: name.to_string(),
                description: None,
                base_image: BaseImageSource::Managed(ManagedImageReference {
                    name: "alpine-minimal".into(),
                    version: "v1".into(),
                    disk: ManagedDiskKind::RootDisk,
                }),
                overlay: root.join(format!("{name}.qcow2")),
                cpus: 1,
                memory: MemorySpec::new("1 GiB", Some(1024 * 1024 * 1024)),
                port_forwards: forwards,
            });
        }

        ProjectConfig {
            file_path: root.join("castra.toml"),
            version: "0.1.0".into(),
            project_name: "demo".into(),
            vms,
            state_root: root.to_path_buf(),
            workflows: Workflows { init: vec![] },
            broker: BrokerConfig { port: broker_port },
            warnings: vec![],
        }
    }

    #[test]
    fn print_port_overview_reports_absence_when_empty() {
        let dir = tempdir().unwrap();
        let project = project_with_forwards(dir.path(), vec![("vm1", vec![])], 7000);
        let (out, err) = super::render_port_overview(&project, false);
        assert!(out.contains("No port forwards declared"));
        assert!(out.contains("VMs without host forwards: vm1"));
        assert!(err.is_empty());
    }

    #[test]
    fn print_port_overview_flags_conflicts_and_broker_collisions() {
        let dir = tempdir().unwrap();
        let forwards = vec![PortForward {
            host: 8080,
            guest: 80,
            protocol: PortProtocol::Tcp,
        }];
        let project = project_with_forwards(
            dir.path(),
            vec![("vm1", forwards.clone()), ("vm2", forwards)],
            8080,
        );
        let (out, err) = super::render_port_overview(&project, false);
        assert!(out.contains("conflict"));
        assert!(err.contains("Warning: host port 8080 is declared by multiple VMs"));
        assert!(err.contains("overlaps with the castra broker"));
    }
}
