use std::collections::HashMap;
use std::ffi::OsStr;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use crate::config::{
    BaseImageSource, BootstrapConfig, BootstrapMode, BrokerConfig,
    DEFAULT_BOOTSTRAP_HANDSHAKE_WAIT_SECS, DEFAULT_BROKER_PORT, LifecycleConfig, ManagedDiskKind,
    ManagedImageReference, MemorySpec, PortConflict, ProjectConfig, VmBootstrapConfig,
    VmDefinition, Workflows,
};
use crate::error::{Error, Result};

use super::diagnostics::{Diagnostic, Severity};
use super::options::{ConfigLoadOptions, ConfigSource, InitOptions};

/// Result of loading a project configuration.
#[derive(Debug)]
pub struct ProjectLoad {
    pub config: ProjectConfig,
    pub diagnostics: Vec<Diagnostic>,
    pub synthetic: bool,
}

/// Shared projects root used for managed caches.
pub fn default_projects_root() -> PathBuf {
    crate::config::user_home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".castra")
        .join("projects")
}

pub fn preferred_init_target(options: &InitOptions) -> PathBuf {
    match (&options.output_path, &options.config_hint) {
        (Some(path), _) => path.clone(),
        (None, ConfigSource::Explicit(path)) => path.clone(),
        (None, ConfigSource::Discover) => PathBuf::from("castra.toml"),
    }
}

pub fn load_project(options: &ConfigLoadOptions) -> Result<ProjectLoad> {
    match resolve_config_path(&options.source, options.search_root.as_ref()) {
        Ok(path) => {
            let config = crate::config::load_project_config(&path)?;
            let diagnostics = config
                .warnings
                .iter()
                .map(|warning| Diagnostic::new(Severity::Warning, warning).with_path(path.clone()))
                .collect();
            Ok(ProjectLoad {
                config,
                diagnostics,
                synthetic: false,
            })
        }
        Err(Error::ConfigDiscoveryFailed { search_root }) if options.allow_synthetic => {
            let synthetic = synthesize_default_project(search_root.clone());
            Ok(ProjectLoad {
                diagnostics: vec![Diagnostic::new(
                    Severity::Info,
                    "Using synthetic configuration – run `castra init` to persist a project.",
                )],
                config: synthetic,
                synthetic: true,
            })
        }
        Err(err) => Err(err),
    }
}

pub fn config_state_root(project: &ProjectConfig) -> PathBuf {
    project.state_root.clone()
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
# See CONFIG.md for the full schema reference.
version = "0.2.0"

[project]
name = "{project_name}"
# state_dir = ".castra/state"  # Uncomment to keep VM state alongside this config

[broker]
# port = 7070

[lifecycle]
# graceful_shutdown_wait_secs = 20
# sigterm_wait_secs = 10
# sigkill_wait_secs = 5

[[vms]]
name = "devbox"
description = "Primary development VM"
base_image = "images/devbox-base.qcow2"
overlay = ".castra/devbox/overlay.qcow2"
cpus = 2
memory = "2048 MiB"
# count = 1  # Increase to scale replicas: devbox-0, devbox-1, ...

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
    source: &ConfigSource,
    search_root: Option<&PathBuf>,
) -> Result<PathBuf> {
    match source {
        ConfigSource::Explicit(path) => {
            if path.is_file() {
                Ok(path.clone())
            } else {
                Err(Error::ExplicitConfigMissing { path: path.clone() })
            }
        }
        ConfigSource::Discover => {
            let cwd = match search_root {
                Some(root) => root.clone(),
                None => current_dir()?,
            };
            discover_config(&cwd).ok_or_else(|| Error::ConfigDiscoveryFailed { search_root: cwd })
        }
    }
}

fn synthesize_default_project(search_root: PathBuf) -> ProjectConfig {
    let synthetic_path = search_root.join("castra.toml");
    let project_name = default_project_name(&synthetic_path);
    let state_root = crate::config::default_state_root(&project_name, &synthetic_path);
    let project_root = synthetic_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| search_root.clone());

    let overlay_path = state_root.join("alpine-minimal-overlay.qcow2");
    let bootstrap_dir = project_root.join("bootstrap").join("alpine-0");

    let vm = VmDefinition {
        name: "alpine-0".to_string(),
        role_name: "alpine".to_string(),
        replica_index: 0,
        description: Some("Managed Alpine Linux guest".to_string()),
        base_image: BaseImageSource::Managed(ManagedImageReference {
            name: "alpine-minimal".to_string(),
            version: "v1".to_string(),
            disk: ManagedDiskKind::RootDisk,
            checksum: None,
            size_bytes: None,
        }),
        overlay: overlay_path,
        cpus: 2,
        memory: MemorySpec::new("2048 MiB", Some(2048 * 1024 * 1024)),
        port_forwards: Vec::new(),
        bootstrap: VmBootstrapConfig {
            mode: BootstrapMode::Auto,
            script: Some(bootstrap_dir.join("run.sh")),
            payload: Some(bootstrap_dir.join("payload")),
            handshake_timeout_secs: DEFAULT_BOOTSTRAP_HANDSHAKE_WAIT_SECS,
            remote_dir: PathBuf::from("/tmp/castra-bootstrap"),
            env: HashMap::new(),
            verify: None,
        },
    };

    ProjectConfig {
        file_path: synthetic_path,
        project_root,
        version: "0.2.0".to_string(),
        project_name,
        vms: vec![vm],
        state_root,
        workflows: Workflows { init: Vec::new() },
        broker: BrokerConfig {
            port: DEFAULT_BROKER_PORT,
        },
        lifecycle: LifecycleConfig::default(),
        bootstrap: BootstrapConfig::default(),
        warnings: vec![],
    }
}

fn current_dir() -> Result<PathBuf> {
    std::env::current_dir().map_err(|source| Error::WorkingDirectoryUnavailable { source })
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

pub fn format_config_warnings(warnings: &[Diagnostic]) -> Option<String> {
    let relevant: Vec<&Diagnostic> = warnings
        .iter()
        .filter(|diag| matches!(diag.severity, Severity::Warning))
        .collect();
    if relevant.is_empty() {
        return None;
    }

    let count = relevant.len();
    let suffix = if count == 1 { "" } else { "s" };
    let mut buf = String::new();
    writeln!(
        buf,
        "Found {count} warning{suffix} while parsing configuration:"
    )
    .unwrap();
    for warning in &relevant {
        writeln!(buf, "  • {}", warning.message).unwrap();
    }
    writeln!(buf, "Next checks:").unwrap();
    writeln!(buf, "  • Review port mappings via `castra ports`.").unwrap();
    writeln!(
        buf,
        "  • Inspect VM status with `castra status` once VMs are running."
    )
    .unwrap();
    buf.push('\n');
    Some(buf)
}

pub fn port_conflicts(conflicts: &[PortConflict]) -> Vec<Diagnostic> {
    conflicts
        .iter()
        .map(|conflict| {
            let message = format!(
                "Port {} is declared by VMs: {}.",
                conflict.port,
                conflict.vm_names.join(", ")
            );
            Diagnostic::new(Severity::Warning, message)
        })
        .collect()
}
