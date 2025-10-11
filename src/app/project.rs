use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use crate::config::{
    BaseImageSource, BrokerConfig, DEFAULT_BROKER_PORT, ManagedDiskKind, ManagedImageReference,
    MemorySpec, ProjectConfig, VmDefinition, Workflows,
};
use crate::error::{CliError, CliResult};

pub fn emit_config_warnings(warnings: &[String]) {
    if warnings.is_empty() {
        return;
    }

    let count = warnings.len();
    let suffix = if count == 1 { "" } else { "s" };
    eprintln!("Found {count} warning{suffix} while parsing configuration:");
    for warning in warnings {
        eprintln!("  • {warning}");
    }
    eprintln!("Next checks:");
    eprintln!("  • Review port mappings via `castra ports`.");
    eprintln!("  • Inspect VM status with `castra status` once VMs are running.");
    eprintln!();
}

pub fn config_root(project: &ProjectConfig) -> PathBuf {
    project
        .file_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn config_state_root(project: &ProjectConfig) -> PathBuf {
    config_root(project).join(".castra")
}

pub fn preferred_config_target(
    config_override: Option<&PathBuf>,
    output_flag: Option<&PathBuf>,
) -> PathBuf {
    if let Some(path) = output_flag {
        return path.clone();
    }
    if let Some(path) = config_override {
        return path.clone();
    }
    PathBuf::from("castra.toml")
}

pub fn load_or_default_project(
    config_override: Option<&PathBuf>,
    skip_discovery: bool,
) -> CliResult<ProjectConfig> {
    match resolve_config_path(config_override, skip_discovery) {
        Ok(path) => crate::config::load_project_config(&path),
        Err(CliError::ConfigDiscoveryFailed { search_root })
            if !skip_discovery && config_override.is_none() =>
        {
            synthesize_default_project(search_root)
        }
        Err(err) => Err(err),
    }
}

fn synthesize_default_project(search_root: PathBuf) -> CliResult<ProjectConfig> {
    let synthetic_path = search_root.join("castra.toml");
    let project_name = default_project_name(&synthetic_path);
    let state_root = synthetic_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".castra");

    let overlay_path = state_root.join("alpine-minimal-overlay.qcow2");

    let vm = VmDefinition {
        name: "alpine".to_string(),
        description: Some("Managed Alpine Linux guest".to_string()),
        base_image: BaseImageSource::Managed(ManagedImageReference {
            name: "alpine-minimal".to_string(),
            version: "v1".to_string(),
            disk: ManagedDiskKind::RootDisk,
        }),
        overlay: overlay_path,
        cpus: 2,
        memory: MemorySpec::new("2048 MiB", Some(2048 * 1024 * 1024)),
        port_forwards: Vec::new(),
    };

    Ok(ProjectConfig {
        file_path: synthetic_path,
        version: "0.1.0".to_string(),
        project_name,
        vms: vec![vm],
        workflows: Workflows { init: Vec::new() },
        broker: BrokerConfig {
            port: DEFAULT_BROKER_PORT,
        },
        warnings: vec![],
    })
}

pub fn default_project_name(target_path: &Path) -> String {
    if let Some(parent) = target_path.parent().filter(|p| !p.as_os_str().is_empty()) {
        if let Some(name) = parent.file_name().and_then(OsStr::to_str) {
            if !name.is_empty() {
                return name.to_string();
            }
        }
    }

    std::env::current_dir()
        .ok()
        .and_then(|path| {
            path.file_name()
                .and_then(OsStr::to_str)
                .map(ToString::to_string)
        })
        .unwrap_or_else(|| "castra-project".to_string())
}

pub fn default_config_contents(project_name: &str) -> String {
    format!(
        r#"# Castra project configuration
# Visit todo_project_config_and_discovery.md for the roadmap.
version = "0.1.0"

[project]
name = "{project_name}"

[[vms]]
name = "devbox"
description = "Primary development VM"
base_image = "images/devbox-base.qcow2"
overlay = ".castra/devbox-overlay.qcow2"
cpus = 2
memory = "2048 MiB"

  [[vms.port_forwards]]
  host = 2222
  guest = 22
  protocol = "tcp"

  [[vms.port_forwards]]
  host = 8080
  guest = 80
  protocol = "tcp"

[workflows]
init = ["qemu-img create -f qcow2 -b {{{{ base_image }}}} {{{{ overlay }}}}"]
"#
    )
}

pub fn resolve_config_path(
    config_override: Option<&PathBuf>,
    skip_discovery: bool,
) -> CliResult<PathBuf> {
    if let Some(path) = config_override {
        if path.is_file() {
            return Ok(path.clone());
        } else {
            return Err(CliError::ExplicitConfigMissing { path: path.clone() });
        }
    }

    if skip_discovery {
        let cwd = current_dir()?;
        return Err(CliError::ConfigDiscoveryFailed { search_root: cwd });
    }

    let cwd = current_dir()?;
    discover_config(&cwd).ok_or_else(|| CliError::ConfigDiscoveryFailed { search_root: cwd })
}

fn current_dir() -> CliResult<PathBuf> {
    std::env::current_dir().map_err(|source| CliError::WorkingDirectoryUnavailable { source })
}

fn discover_config(start: &Path) -> Option<PathBuf> {
    let mut cursor = Some(start.to_path_buf());
    while let Some(dir) = cursor {
        let candidate = dir.join("castra.toml");
        if candidate.is_file() {
            return Some(candidate);
        }
        cursor = dir.parent().map(Path::to_path_buf);
    }
    None
}
