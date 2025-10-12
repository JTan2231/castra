use std::ffi::OsStr;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use crate::config::{
    BaseImageSource, BrokerConfig, DEFAULT_BROKER_PORT, ManagedDiskKind, ManagedImageReference,
    MemorySpec, ProjectConfig, VmDefinition, Workflows,
};
use crate::error::{CliError, CliResult};

pub fn emit_config_warnings(warnings: &[String]) {
    if let Some(message) = format_config_warnings(warnings) {
        eprint!("{message}");
    }
}

fn format_config_warnings(warnings: &[String]) -> Option<String> {
    if warnings.is_empty() {
        return None;
    }

    let count = warnings.len();
    let suffix = if count == 1 { "" } else { "s" };
    let mut buf = String::new();
    writeln!(
        buf,
        "Found {count} warning{suffix} while parsing configuration:"
    )
    .unwrap();
    for warning in warnings {
        writeln!(buf, "  • {warning}").unwrap();
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

pub fn config_state_root(project: &ProjectConfig) -> PathBuf {
    project.state_root.clone()
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
    let state_root = crate::config::default_state_root(&project_name, &synthetic_path);

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
        state_root,
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
# state_dir = ".castra"  # Uncomment to keep VM state alongside this config

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::CliError;
    use std::fs;
    use std::path::Path;
    use tempfile::tempdir;

    #[test]
    fn emit_config_warnings_prints_bullets() {
        let output = super::format_config_warnings(&["first".to_string(), "second".to_string()])
            .expect("warnings should generate output");
        assert!(output.contains("Found 2 warnings"));
        assert!(output.contains("  • first"));
        assert!(output.contains("  • second"));
    }

    #[test]
    fn emit_config_warnings_silent_when_empty() {
        assert!(super::format_config_warnings(&[]).is_none());
    }

    #[test]
    fn preferred_config_target_prioritizes_output() {
        let config = PathBuf::from("config.toml");
        let output = PathBuf::from("out.toml");
        let target = preferred_config_target(Some(&config), Some(&output));
        assert_eq!(target, output);

        let target = preferred_config_target(Some(&config), None);
        assert_eq!(target, config);

        let target = preferred_config_target(None, None);
        assert_eq!(target, PathBuf::from("castra.toml"));
    }

    #[test]
    fn default_project_name_uses_parent_dir() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("castra.toml");
        let name = default_project_name(&path);
        let expected = dir
            .path()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();
        assert_eq!(name, expected);
    }

    #[test]
    fn default_project_name_falls_back_to_cwd() {
        let original = std::env::current_dir().unwrap();
        let dir = tempdir().unwrap();
        std::env::set_current_dir(dir.path()).unwrap();
        let name = default_project_name(Path::new("castra.toml"));
        std::env::set_current_dir(&original).unwrap();
        let expected = dir
            .path()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();
        assert_eq!(name, expected);
    }

    #[test]
    fn resolve_config_path_respects_override() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("custom.toml");
        fs::write(&path, "version = \"0.1.0\"").unwrap();
        let resolved =
            resolve_config_path(Some(&path), false).expect("explicit path should succeed");
        assert_eq!(resolved, path);
    }

    #[test]
    fn resolve_config_path_errors_when_missing() {
        let path = PathBuf::from("/tmp/does-not-exist.toml");
        let err = resolve_config_path(Some(&path), false).unwrap_err();
        match err {
            CliError::ExplicitConfigMissing { path: missing } => {
                assert_eq!(missing, path);
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn load_or_default_project_synthesizes_when_missing() {
        let dir = tempdir().unwrap();
        let original = std::env::current_dir().unwrap();
        std::env::set_current_dir(dir.path()).unwrap();
        let project =
            load_or_default_project(None, false).expect("fallback project should be synthesized");
        std::env::set_current_dir(&original).unwrap();
        let expected = dir
            .path()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();
        assert_eq!(project.project_name, expected);
        assert_eq!(project.vms.len(), 1);
        assert!(
            matches!(
                project.vms[0].base_image,
                BaseImageSource::Managed(ManagedImageReference { .. })
            ),
            "expected managed image in synthesized project"
        );
    }

    #[test]
    fn config_state_root_clones_path() {
        let project = ProjectConfig {
            file_path: PathBuf::from("castra.toml"),
            version: "0.1.0".into(),
            project_name: "demo".into(),
            vms: vec![],
            state_root: PathBuf::from("/state"),
            workflows: Workflows { init: vec![] },
            broker: BrokerConfig {
                port: DEFAULT_BROKER_PORT,
            },
            warnings: vec![],
        };
        let root = config_state_root(&project);
        assert_eq!(root, PathBuf::from("/state"));
    }
}
