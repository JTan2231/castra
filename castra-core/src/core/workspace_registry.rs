use std::collections::{BTreeMap, HashSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::config::{self, ProjectConfig};
use crate::core::diagnostics::{Diagnostic, Severity};
use crate::core::options::{ConfigLoadOptions, ConfigSource, UpOptions, VmLaunchMode};
use crate::core::project::default_projects_root;
use crate::core::runtime::inspect_vm_state;
use crate::error::{Error, Result};

use sha2::{Digest, Sha256};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

pub fn persist_workspace_metadata(
    project: &ProjectConfig,
    synthetic_config: bool,
    options: &UpOptions,
    state_root: &Path,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<()> {
    let metadata_dir = state_root.join("metadata");
    fs::create_dir_all(&metadata_dir).map_err(|err| Error::PreflightFailed {
        message: format!(
            "Failed to prepare workspace metadata directory at {}: {err}",
            metadata_dir.display()
        ),
    })?;

    let config_contents = match fs::read_to_string(&project.file_path) {
        Ok(contents) => Some(contents),
        Err(err) => {
            diagnostics.push(
                Diagnostic::new(
                    Severity::Warning,
                    format!(
                        "Unable to read configuration at {} for snapshotting: {err}",
                        project.file_path.display()
                    ),
                )
                .with_help("Registry fallback will rely on the cached snapshot under <state_root>/metadata/config_snapshot.toml."),
            );
            None
        }
    };

    let snapshot_path = metadata_dir.join("config_snapshot.toml");
    let mut config_digest = None;
    let mut snapshot_ref: Option<PathBuf> = None;

    if let Some(contents) = config_contents.as_ref() {
        config_digest = Some(hash_config(contents));
        fs::write(&snapshot_path, contents).map_err(|err| Error::PreflightFailed {
            message: format!(
                "Failed to write configuration snapshot at {}: {err}",
                snapshot_path.display()
            ),
        })?;
        snapshot_ref = Some(snapshot_path.clone());
    } else if snapshot_path.is_file() {
        match fs::read_to_string(&snapshot_path) {
            Ok(existing) => {
                config_digest = Some(hash_config(&existing));
                snapshot_ref = Some(snapshot_path.clone());
            }
            Err(err) => diagnostics.push(
                Diagnostic::new(
                    Severity::Warning,
                    format!(
                        "Existing configuration snapshot at {} is unreadable: {err}",
                        snapshot_path.display()
                    ),
                )
                .with_help("Delete the corrupted snapshot and rerun `castra up` to regenerate it."),
            ),
        }
    }

    let metadata = build_workspace_metadata(
        project,
        synthetic_config,
        options,
        state_root,
        config_digest.clone(),
        snapshot_ref.clone(),
    );

    let metadata_json =
        serde_json::to_string_pretty(&metadata).map_err(|err| Error::PreflightFailed {
            message: format!(
                "Failed to serialize workspace metadata for project {}: {err}",
                project.project_name
            ),
        })?;

    let workspace_json = metadata_dir.join("workspace.json");
    fs::write(&workspace_json, &metadata_json).map_err(|err| Error::PreflightFailed {
        message: format!(
            "Failed to write workspace metadata at {}: {err}",
            workspace_json.display()
        ),
    })?;

    let config_metadata_json = metadata_dir.join("config_metadata.json");
    fs::write(&config_metadata_json, &metadata_json).map_err(|err| Error::PreflightFailed {
        message: format!(
            "Failed to write config metadata at {}: {err}",
            config_metadata_json.display()
        ),
    })?;

    if config_contents.is_none() && snapshot_ref.is_none() {
        diagnostics.push(
            Diagnostic::new(
                Severity::Warning,
                "Workspace metadata written without a configuration snapshot; registry rehydration will rely on an accessible castra.toml.",
            )
            .with_help("Ensure castra.toml is accessible or rerun `castra up` once restored."),
        );
    } else if config_contents.is_none() && config_digest.is_some() {
        diagnostics.push(
            Diagnostic::new(
                Severity::Info,
                "Using existing configuration snapshot cached under metadata/config_snapshot.toml.",
            )
            .with_help(
                "Keep the snapshot in sync by rerunning `castra up` after restoring castra.toml.",
            ),
        );
    }

    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceMetadata {
    pub metadata_version: String,
    pub recorded_at: String,
    pub project: ProjectMetadata,
    pub workspace: WorkspaceInfoMetadata,
    pub config: ConfigMetadata,
    pub bootstrap: BootstrapMetadata,
    pub invocation: InvocationMetadata,
    #[serde(default)]
    pub vms: Vec<WorkspaceVmMetadata>,
    #[serde(default)]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectMetadata {
    pub name: String,
    pub version: String,
    pub root: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceInfoMetadata {
    pub id: String,
    pub state_root: PathBuf,
    #[serde(default)]
    pub label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigMetadata {
    pub path: PathBuf,
    pub source: ConfigSourceMetadata,
    #[serde(default)]
    pub digest_sha256: Option<String>,
    #[serde(default)]
    pub snapshot_path: Option<PathBuf>,
    #[serde(default)]
    pub synthetic: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigSourceMetadata {
    pub kind: String,
    #[serde(default)]
    pub explicit_path: Option<PathBuf>,
    #[serde(default)]
    pub search_root: Option<PathBuf>,
    pub allow_synthetic: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootstrapMetadata {
    pub global_mode: String,
    pub overrides: BootstrapOverridesMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BootstrapOverridesMetadata {
    #[serde(default)]
    pub global: Option<String>,
    #[serde(default)]
    pub per_vm: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvocationMetadata {
    pub plan: bool,
    pub force: bool,
    #[serde(default)]
    pub alpine_qcow_override: Option<PathBuf>,
    pub bootstrap_overrides_applied: bool,
    #[serde(default = "default_vm_launch_mode_descriptor")]
    pub vm_launch_mode: String,
}

fn default_vm_launch_mode_descriptor() -> String {
    VmLaunchMode::Daemonize.as_str().to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceVmMetadata {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub bootstrap_mode: String,
    pub overlay_path: PathBuf,
    pub base_image: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigSnapshotSource {
    ConfigFile,
    CachedSnapshot,
    Unavailable,
}

#[derive(Debug, Clone)]
pub struct WorkspaceHandle {
    pub workspace_id: String,
    pub state_root: PathBuf,
    pub metadata: Option<WorkspaceMetadata>,
    pub metadata_path: Option<PathBuf>,
    pub config_snapshot_source: ConfigSnapshotSource,
    pub config_path: Option<PathBuf>,
    pub snapshot_path: Option<PathBuf>,
    pub project_name: Option<String>,
    pub config_version: Option<String>,
    pub runtime: WorkspaceRuntimeState,
    pub active: bool,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone)]
pub struct WorkspaceRuntimeState {
    pub vms: Vec<VmRuntimeState>,
}

impl WorkspaceRuntimeState {
    pub fn is_active(&self) -> bool {
        self.vms.iter().any(|vm| vm.running)
    }
}

impl WorkspaceHandle {
    pub fn load_project_config(&self) -> Result<ProjectConfig> {
        if let Some(path) = self.config_path.as_ref().filter(|path| path.is_file()) {
            let mut project = config::load_project_config(path)?;
            project.state_root = self.state_root.clone();
            return Ok(project);
        }

        if let Some(snapshot) = self.snapshot_path.as_ref().filter(|path| path.is_file()) {
            let mut project = config::load_project_config(snapshot)?;
            if let Some(metadata) = &self.metadata {
                project.file_path = metadata.config.path.clone();
                project.project_root = metadata.project.root.clone();
            } else if let Some(config_path) = &self.config_path {
                project.file_path = config_path.clone();
                if let Some(root) = config_path.parent() {
                    project.project_root = root.to_path_buf();
                }
            }
            project.state_root = self.state_root.clone();
            return Ok(project);
        }

        Err(Error::WorkspaceConfigUnavailable {
            id: self.workspace_id.clone(),
        })
    }
}

#[derive(Debug, Clone)]
pub struct VmRuntimeState {
    pub name: String,
    pub state: String,
    pub running: bool,
    pub uptime: Option<Duration>,
}

#[derive(Debug)]
pub struct WorkspaceRegistry {
    roots: Vec<PathBuf>,
    entries: Vec<WorkspaceHandle>,
    diagnostics: Vec<Diagnostic>,
}

impl WorkspaceRegistry {
    pub fn discover() -> Result<Self> {
        let roots = collect_workspace_roots();
        let mut registry = Self {
            roots,
            entries: Vec::new(),
            diagnostics: Vec::new(),
        };
        registry.refresh()?;
        Ok(registry)
    }

    pub fn refresh(&mut self) -> Result<()> {
        self.entries.clear();
        self.diagnostics.clear();

        let mut seen = HashSet::new();
        for root in &self.roots {
            for candidate in enumerate_state_roots(root, &mut self.diagnostics) {
                let canonical = canonicalize_if_possible(&candidate);
                if !seen.insert(canonical.clone()) {
                    continue;
                }
                let handle = inspect_workspace(&canonical, &mut self.diagnostics);
                self.entries.push(handle);
            }
        }

        self.entries.sort_by(|a, b| a.state_root.cmp(&b.state_root));

        Ok(())
    }

    pub fn entries(&self) -> &[WorkspaceHandle] {
        &self.entries
    }

    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    pub fn list_active(&self) -> Vec<&WorkspaceHandle> {
        self.entries.iter().filter(|entry| entry.active).collect()
    }

    pub fn find_by_config(&self, config_path: &Path) -> Option<&WorkspaceHandle> {
        self.entries.iter().find(|entry| {
            entry
                .config_path
                .as_ref()
                .map(|path| paths_equal(path, config_path))
                .unwrap_or(false)
        })
    }
}

fn build_workspace_metadata(
    project: &ProjectConfig,
    synthetic_config: bool,
    options: &UpOptions,
    state_root: &Path,
    config_digest: Option<String>,
    snapshot_path: Option<PathBuf>,
) -> WorkspaceMetadata {
    let recorded_at = OffsetDateTime::now_utc();
    let recorded_at = recorded_at
        .format(&Rfc3339)
        .unwrap_or_else(|_| recorded_at.to_string());

    let workspace_id = derive_workspace_id(state_root);
    let workspace_label = state_root
        .file_name()
        .and_then(|value| value.to_str())
        .map(ToString::to_string);

    let per_vm_overrides: BTreeMap<String, String> = options
        .bootstrap
        .per_vm
        .iter()
        .map(|(name, mode)| (name.clone(), mode.as_str().to_string()))
        .collect();

    let vm_entries = project
        .vms
        .iter()
        .map(|vm| WorkspaceVmMetadata {
            name: vm.name.clone(),
            description: vm.description.clone(),
            bootstrap_mode: vm.bootstrap.mode.as_str().to_string(),
            overlay_path: vm.overlay.clone(),
            base_image: vm.base_image.describe(),
        })
        .collect();

    let mut notes = vec!["Guest disk changes are ephemeral; export via SSH before running `castra down` if you need to retain data.".to_string()];
    if synthetic_config {
        notes.push(
            "Workspace seeded from a synthetic configuration; run `castra init` to persist castra.toml."
                .to_string(),
        );
    }

    WorkspaceMetadata {
        metadata_version: "1".to_string(),
        recorded_at,
        project: ProjectMetadata {
            name: project.project_name.clone(),
            version: project.version.clone(),
            root: project.project_root.clone(),
        },
        workspace: WorkspaceInfoMetadata {
            id: workspace_id,
            state_root: state_root.to_path_buf(),
            label: workspace_label,
        },
        config: ConfigMetadata {
            path: project.file_path.clone(),
            source: build_config_source_metadata(&options.config),
            digest_sha256: config_digest,
            snapshot_path,
            synthetic: synthetic_config,
        },
        bootstrap: BootstrapMetadata {
            global_mode: project.bootstrap.mode.as_str().to_string(),
            overrides: BootstrapOverridesMetadata {
                global: options
                    .bootstrap
                    .global
                    .map(|mode| mode.as_str().to_string()),
                per_vm: per_vm_overrides,
            },
        },
        invocation: InvocationMetadata {
            plan: options.plan,
            force: options.force,
            alpine_qcow_override: options.alpine_qcow_override.clone(),
            bootstrap_overrides_applied: options.bootstrap.global.is_some()
                || !options.bootstrap.per_vm.is_empty(),
            vm_launch_mode: options.launch_mode.as_str().to_string(),
        },
        vms: vm_entries,
        notes,
    }
}

fn build_config_source_metadata(options: &ConfigLoadOptions) -> ConfigSourceMetadata {
    match &options.source {
        ConfigSource::Explicit(path) => ConfigSourceMetadata {
            kind: "explicit".to_string(),
            explicit_path: Some(path.clone()),
            search_root: options.search_root.clone(),
            allow_synthetic: options.allow_synthetic,
        },
        ConfigSource::Discover => ConfigSourceMetadata {
            kind: "discover".to_string(),
            explicit_path: None,
            search_root: options.search_root.clone(),
            allow_synthetic: options.allow_synthetic,
        },
    }
}

fn inspect_workspace(state_root: &Path, registry_diags: &mut Vec<Diagnostic>) -> WorkspaceHandle {
    let metadata_candidates = [
        state_root.join("metadata").join("workspace.json"),
        state_root.join("metadata").join("config_metadata.json"),
    ];

    let mut metadata = None;
    let mut metadata_path = None;
    let mut diagnostics = Vec::new();

    for candidate in metadata_candidates {
        if !candidate.is_file() {
            continue;
        }
        match fs::read_to_string(&candidate) {
            Ok(contents) => match serde_json::from_str::<WorkspaceMetadata>(&contents) {
                Ok(parsed) => {
                    metadata = Some(parsed);
                    metadata_path = Some(candidate);
                    break;
                }
                Err(err) => diagnostics.push(
                    Diagnostic::new(
                        Severity::Warning,
                        format!(
                            "Failed to parse workspace metadata at {}: {err}",
                            candidate.display()
                        ),
                    )
                    .with_help("Delete corrupted metadata and rerun `castra up` to regenerate it."),
                ),
            },
            Err(err) => diagnostics.push(
                Diagnostic::new(
                    Severity::Warning,
                    format!(
                        "Unable to read workspace metadata at {}: {err}",
                        candidate.display()
                    ),
                )
                .with_help("Check permissions or delete the metadata directory before rerunning `castra up`."),
            ),
        }
    }

    let snapshot_path = match &metadata {
        Some(meta) => meta.config.snapshot_path.clone(),
        None => {
            let candidate = state_root.join("metadata").join("config_snapshot.toml");
            candidate.is_file().then_some(candidate)
        }
    };

    let config_path = metadata.as_ref().map(|meta| meta.config.path.clone());

    let config_snapshot_source = match (&config_path, &snapshot_path) {
        (Some(path), _) if path.is_file() => ConfigSnapshotSource::ConfigFile,
        (_, Some(path)) if path.is_file() => ConfigSnapshotSource::CachedSnapshot,
        _ => ConfigSnapshotSource::Unavailable,
    };

    let runtime = inspect_runtime(state_root, metadata.as_ref(), &mut diagnostics);
    let active = runtime.is_active();

    let workspace_id = metadata
        .as_ref()
        .map(|meta| meta.workspace.id.clone())
        .unwrap_or_else(|| derive_workspace_id(state_root));

    let project_name = metadata
        .as_ref()
        .map(|meta| meta.project.name.clone())
        .or_else(|| {
            state_root
                .file_name()
                .and_then(|name| name.to_str().map(str::to_string))
        });

    let config_version = metadata.as_ref().map(|meta| meta.project.version.clone());

    registry_diags.extend(diagnostics.iter().cloned());

    WorkspaceHandle {
        workspace_id,
        state_root: state_root.to_path_buf(),
        metadata,
        metadata_path,
        config_snapshot_source,
        config_path,
        snapshot_path,
        project_name,
        config_version,
        runtime,
        active,
        diagnostics,
    }
}

fn inspect_runtime(
    state_root: &Path,
    metadata: Option<&WorkspaceMetadata>,
    diagnostics: &mut Vec<Diagnostic>,
) -> WorkspaceRuntimeState {
    let vm_names: Vec<String> = if let Some(meta) = metadata {
        meta.vms.iter().map(|vm| vm.name.clone()).collect()
    } else {
        discover_vm_names_from_pidfiles(state_root)
    };

    let mut vm_states = Vec::new();
    for vm_name in vm_names {
        let pidfile = state_root.join(format!("{vm_name}.pid"));
        let (state, uptime, mut warnings) = inspect_vm_state(&pidfile, &vm_name);
        diagnostics.extend(
            warnings
                .drain(..)
                .map(|warning| Diagnostic::new(Severity::Warning, warning)),
        );
        let running = state == "running";
        vm_states.push(VmRuntimeState {
            name: vm_name,
            state,
            running,
            uptime,
        });
    }

    WorkspaceRuntimeState { vms: vm_states }
}

fn discover_vm_names_from_pidfiles(state_root: &Path) -> Vec<String> {
    let mut names = Vec::new();
    if let Ok(entries) = fs::read_dir(state_root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            if let Some(name) = path.file_name().and_then(|name| name.to_str()) {
                if let Some(stripped) = name.strip_suffix(".pid") {
                    if stripped != "broker" && !stripped.is_empty() {
                        names.push(stripped.to_string());
                    }
                }
            }
        }
    }
    names.sort();
    names
}

fn collect_workspace_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();

    if let Some(raw) = env::var_os("CASTRA_WORKSPACE_ROOTS") {
        for path in env::split_paths(&raw) {
            if !path.as_os_str().is_empty() {
                roots.push(path);
            }
        }
    }

    roots.push(default_projects_root());

    if let Ok(cwd) = env::current_dir() {
        let local = cwd.join(".castra");
        if local.is_dir() {
            roots.push(local);
        }
        let local_state = cwd.join(".castra").join("state");
        if local_state.is_dir() {
            roots.push(local_state);
        }
    }

    deduplicate_paths(roots)
}

fn enumerate_state_roots(root: &Path, diagnostics: &mut Vec<Diagnostic>) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if looks_like_state_root(root) {
        candidates.push(root.to_path_buf());
    }

    match fs::read_dir(root) {
        Ok(entries) => {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() && looks_like_state_root(&path) {
                    candidates.push(path);
                }
            }
        }
        Err(err) if root.exists() => diagnostics.push(
            Diagnostic::new(
                Severity::Warning,
                format!("Failed to inspect workspace root {}: {err}", root.display()),
            )
            .with_help("Check permissions or adjust CASRA_WORKSPACE_ROOTS."),
        ),
        _ => {}
    }

    deduplicate_paths(candidates)
}

fn looks_like_state_root(path: &Path) -> bool {
    path.join("metadata").is_dir()
        || path.join("broker.pid").is_file()
        || path.join("handshakes").is_dir()
        || path.join("logs").is_dir()
}

fn deduplicate_paths(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut unique = Vec::new();
    for path in paths {
        let candidate = canonicalize_if_possible(&path);
        if unique.iter().any(|existing| existing == &candidate) {
            continue;
        }
        unique.push(candidate);
    }
    unique
}

fn canonicalize_if_possible(path: &Path) -> PathBuf {
    if path.exists() {
        match path.canonicalize() {
            Ok(canonical) => canonical,
            Err(_) => path.to_path_buf(),
        }
    } else {
        path.to_path_buf()
    }
}

fn paths_equal(a: &Path, b: &Path) -> bool {
    let ca = canonicalize_if_possible(a);
    let cb = canonicalize_if_possible(b);
    ca == cb
}

fn derive_workspace_id(state_root: &Path) -> String {
    let mut hasher = Sha256::new();
    hasher.update(state_root.to_string_lossy().as_bytes());
    let digest = hasher.finalize();
    hex::encode(&digest[..8])
}

fn hash_config(contents: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(contents.as_bytes());
    let digest = hasher.finalize();
    hex::encode(digest)
}
