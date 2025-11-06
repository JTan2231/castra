use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::Duration;
use std::{env, fs};

use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::error::Error;

pub const DEFAULT_IMAGE_SUBDIR: &str = "images";
pub const DEFAULT_ALPINE_IMAGE_FILENAME: &str = "alpine-x86_64.qcow2";
const DEFAULT_OVERLAY_SUBDIR: &str = "overlays";
const DEFAULT_OVERLAY_SUFFIX: &str = "overlay";
const DEFAULT_OVERLAY_EXTENSION: &str = "qcow2";

pub const DEFAULT_GRACEFUL_SHUTDOWN_WAIT_SECS: u64 = 20;
pub const DEFAULT_SIGTERM_WAIT_SECS: u64 = 10;
pub const DEFAULT_SIGKILL_WAIT_SECS: u64 = 5;
pub const DEFAULT_BOOTSTRAP_HANDSHAKE_WAIT_SECS: u64 = 120;

pub const BROKERLESS_MIGRATION_DOC: &str = "docs/migration/brokerless-core.md";
#[derive(Debug, Clone)]
pub struct BaseImageSource {
    path: PathBuf,
    provenance: BaseImageProvenance,
}

impl BaseImageSource {
    pub fn describe(&self) -> String {
        self.path.display().to_string()
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn provenance(&self) -> BaseImageProvenance {
        self.provenance
    }

    pub(crate) fn new(path: PathBuf, provenance: BaseImageProvenance) -> Self {
        Self { path, provenance }
    }

    pub fn from_explicit(path: impl Into<PathBuf>) -> Self {
        Self::new(path.into(), BaseImageProvenance::Explicit)
    }

    pub fn from_default_alpine(path: impl Into<PathBuf>) -> Self {
        Self::new(path.into(), BaseImageProvenance::DefaultAlpine)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BaseImageProvenance {
    Explicit,
    DefaultAlpine,
}

#[derive(Debug, Clone)]
pub struct ProjectConfig {
    pub file_path: PathBuf,
    pub project_root: PathBuf,
    pub version: String,
    pub project_name: String,
    pub features: ProjectFeatures,
    pub vms: Vec<VmDefinition>,
    pub state_root: PathBuf,
    pub workflows: Workflows,
    pub lifecycle: LifecycleConfig,
    pub bootstrap: BootstrapConfig,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ProjectFeatures;

#[derive(Debug, Clone)]
pub struct LifecycleConfig {
    pub graceful_shutdown_wait_secs: u64,
    pub sigterm_wait_secs: u64,
    pub sigkill_wait_secs: u64,
}

impl Default for LifecycleConfig {
    fn default() -> Self {
        Self {
            graceful_shutdown_wait_secs: DEFAULT_GRACEFUL_SHUTDOWN_WAIT_SECS,
            sigterm_wait_secs: DEFAULT_SIGTERM_WAIT_SECS,
            sigkill_wait_secs: DEFAULT_SIGKILL_WAIT_SECS,
        }
    }
}

impl LifecycleConfig {
    pub fn graceful_wait(&self) -> Duration {
        Duration::from_secs(self.graceful_shutdown_wait_secs)
    }

    pub fn sigterm_wait(&self) -> Duration {
        Duration::from_secs(self.sigterm_wait_secs)
    }

    pub fn sigkill_wait(&self) -> Duration {
        Duration::from_secs(self.sigkill_wait_secs)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootstrapMode {
    Skip,
    Auto,
    Always,
}

impl BootstrapMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Skip => "skip",
            Self::Auto => "auto",
            Self::Always => "always",
        }
    }
}

impl FromStr for BootstrapMode {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.to_ascii_lowercase().as_str() {
            "skip" | "disabled" | "off" => Ok(Self::Skip),
            "auto" | "automatic" | "enabled" => Ok(Self::Auto),
            "always" | "force" => Ok(Self::Always),
            _ => Err(format!(
                "Unknown bootstrap mode `{value}`. Supported values: auto, skip, always."
            )),
        }
    }
}

impl Default for BootstrapMode {
    fn default() -> Self {
        Self::Auto
    }
}

#[derive(Debug, Clone)]
pub struct BootstrapConfig {
    pub mode: BootstrapMode,
    pub handshake_timeout_secs: u64,
    pub remote_dir: PathBuf,
    pub env: HashMap<String, String>,
}

impl Default for BootstrapConfig {
    fn default() -> Self {
        Self {
            mode: BootstrapMode::default(),
            handshake_timeout_secs: DEFAULT_BOOTSTRAP_HANDSHAKE_WAIT_SECS,
            remote_dir: PathBuf::from("/tmp/castra-bootstrap"),
            env: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct VmBootstrapConfig {
    pub mode: BootstrapMode,
    pub script: Option<PathBuf>,
    pub payload: Option<PathBuf>,
    pub handshake_timeout_secs: u64,
    pub remote_dir: PathBuf,
    pub env: HashMap<String, String>,
    pub verify: Option<BootstrapVerifyConfig>,
}

#[derive(Debug, Clone)]
pub struct BootstrapVerifyConfig {
    pub command: Option<String>,
    pub path: Option<PathBuf>,
}

impl ProjectConfig {
    pub fn port_conflicts(&self) -> Vec<PortConflict> {
        let mut map: HashMap<u16, Vec<&VmDefinition>> = HashMap::new();
        for vm in &self.vms {
            for forward in &vm.port_forwards {
                map.entry(forward.host).or_default().push(vm);
            }
        }

        map.into_iter()
            .filter(|(_, vms)| vms.len() > 1)
            .map(|(port, vms)| PortConflict {
                port,
                vm_names: vms.iter().map(|vm| vm.name.clone()).collect(),
            })
            .collect()
    }
}

#[derive(Debug, Clone)]
pub struct VmDefinition {
    pub name: String,
    pub role_name: String,
    pub replica_index: usize,
    pub description: Option<String>,
    pub base_image: BaseImageSource,
    pub overlay: PathBuf,
    pub cpus: u32,
    pub memory: MemorySpec,
    pub port_forwards: Vec<PortForward>,
    pub bootstrap: VmBootstrapConfig,
}

#[derive(Debug, Clone)]
pub struct MemorySpec {
    original: String,
    bytes: Option<u64>,
}

impl MemorySpec {
    pub fn original(&self) -> &str {
        &self.original
    }

    pub fn bytes(&self) -> Option<u64> {
        self.bytes
    }

    pub(crate) fn new(original: impl Into<String>, bytes: Option<u64>) -> Self {
        Self {
            original: original.into(),
            bytes,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PortForward {
    pub host: u16,
    pub guest: u16,
    pub protocol: PortProtocol,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PortProtocol {
    Tcp,
    Udp,
}

impl PortProtocol {
    fn from_str(input: &str) -> Option<Self> {
        match input.to_ascii_lowercase().as_str() {
            "tcp" => Some(Self::Tcp),
            "udp" => Some(Self::Udp),
            _ => None,
        }
    }
}

impl std::fmt::Display for PortProtocol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PortProtocol::Tcp => write!(f, "tcp"),
            PortProtocol::Udp => write!(f, "udp"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Workflows {
    pub init: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct PortConflict {
    pub port: u16,
    pub vm_names: Vec<String>,
}

pub fn load_project_config(path: &Path) -> Result<ProjectConfig, Error> {
    let contents = fs::read_to_string(path).map_err(|source| Error::ReadConfig {
        path: path.to_path_buf(),
        source,
    })?;

    let value: toml::Value = toml::from_str(&contents).map_err(|source| Error::ParseConfig {
        path: path.to_path_buf(),
        source,
    })?;

    let legacy_keys = find_legacy_broker_keys(&value);
    if !legacy_keys.is_empty() {
        let details = if legacy_keys.len() == 1 {
            format!("remove {} from castra.toml", legacy_keys[0])
        } else {
            format!(
                "remove legacy keys from castra.toml: {}",
                legacy_keys.join(", ")
            )
        };
        return Err(Error::DeprecatedConfig {
            path: path.to_path_buf(),
            details,
            doc: BROKERLESS_MIGRATION_DOC,
        });
    }

    let mut warnings = detect_unknown_fields(&value);

    let raw = RawConfig::deserialize(value).map_err(|source| Error::ParseConfig {
        path: path.to_path_buf(),
        source,
    })?;

    raw.into_validated(path, &mut warnings)
}

fn invalid_config(path: &Path, message: impl Into<String>) -> Error {
    Error::InvalidConfig {
        path: path.to_path_buf(),
        message: message.into(),
    }
}

fn detect_unknown_fields(value: &toml::Value) -> Vec<String> {
    let mut warnings = Vec::new();
    let allowed_root = [
        "version",
        "project",
        "vms",
        "workflows",
        "lifecycle",
        "bootstrap",
    ];

    if let toml::Value::Table(table) = value {
        warn_table(table, &allowed_root, "root", &mut warnings);

        if let Some(project) = table.get("project") {
            if let toml::Value::Table(project_table) = project {
                warn_table(
                    project_table,
                    &["name", "state_dir", "features"],
                    "[project]",
                    &mut warnings,
                );

                if let Some(features) = project_table.get("features") {
                    if let toml::Value::Table(features_table) = features {
                        warn_table(
                            features_table,
                            &["enable_vm_vizier"],
                            "[project.features]",
                            &mut warnings,
                        );
                    } else {
                        warnings.push("Expected [project.features] to be a table.".to_string());
                    }
                }
            } else {
                warnings.push("Expected [project] to be a table.".to_string());
            }
        }

        if let Some(vms) = table.get("vms") {
            if let toml::Value::Array(vm_entries) = vms {
                for (idx, entry) in vm_entries.iter().enumerate() {
                    if let toml::Value::Table(vm_table) = entry {
                        warn_table(
                            vm_table,
                            &[
                                "name",
                                "description",
                                "base_image",
                                "managed_image",
                                "overlay",
                                "cpus",
                                "memory",
                                "port_forwards",
                                "count",
                                "instances",
                                "bootstrap",
                            ],
                            &format!("[[vms]] #{idx}"),
                            &mut warnings,
                        );

                        if let Some(managed_image) = vm_table.get("managed_image") {
                            if let toml::Value::Table(managed_table) = managed_image {
                                warn_table(
                                    managed_table,
                                    &["name", "version", "disk", "checksum", "size_bytes"],
                                    &format!("[[vms]] #{idx}.managed_image"),
                                    &mut warnings,
                                );
                            } else {
                                warnings.push(format!(
                                    "`managed_image` on [[vms]] entry #{idx} must be a table."
                                ));
                            }
                        }

                        if let Some(port_forwards) = vm_table.get("port_forwards") {
                            if let toml::Value::Array(tables) = port_forwards {
                                for (pf_idx, pf) in tables.iter().enumerate() {
                                    if let toml::Value::Table(pf_table) = pf {
                                        warn_table(
                                            pf_table,
                                            &["host", "guest", "protocol"],
                                            &format!("[[vms.port_forwards]] #{pf_idx}"),
                                            &mut warnings,
                                        );
                                    } else {
                                        warnings.push(format!(
                                            "[[vms.port_forwards]] entry #{pf_idx} must be a table."
                                        ));
                                    }
                                }
                            } else {
                                warnings.push(
                                    "`port_forwards` must be an array of tables.".to_string(),
                                );
                            }
                        }

                        if let Some(bootstrap) = vm_table.get("bootstrap") {
                            if let toml::Value::Table(bootstrap_table) = bootstrap {
                                warn_table(
                                    bootstrap_table,
                                    &[
                                        "mode",
                                        "script",
                                        "payload",
                                        "handshake_timeout_secs",
                                        "remote_dir",
                                        "env",
                                        "verify_command",
                                        "verify_path",
                                    ],
                                    &format!("[[vms]] #{idx}.bootstrap"),
                                    &mut warnings,
                                );
                                if let Some(env) = bootstrap_table.get("env") {
                                    if !env.is_table() {
                                        warnings.push(format!(
                                            "`env` on [[vms]] entry #{idx}.bootstrap must be a table mapping keys to values."
                                        ));
                                    }
                                }
                            } else {
                                warnings.push(format!(
                                    "`bootstrap` on [[vms]] entry #{idx} must be a table."
                                ));
                            }
                        }

                        if let Some(instances) = vm_table.get("instances") {
                            if let toml::Value::Array(instance_entries) = instances {
                                for (inst_idx, instance) in instance_entries.iter().enumerate() {
                                    if let toml::Value::Table(instance_table) = instance {
                                        warn_table(
                                            instance_table,
                                            &[
                                                "id",
                                                "description",
                                                "base_image",
                                                "managed_image",
                                                "overlay",
                                                "cpus",
                                                "memory",
                                                "port_forwards",
                                            ],
                                            &format!("[[vms.instances]] #{inst_idx}"),
                                            &mut warnings,
                                        );

                                        if let Some(managed_image) =
                                            instance_table.get("managed_image")
                                        {
                                            if let toml::Value::Table(managed_table) = managed_image
                                            {
                                                warn_table(
                                                    managed_table,
                                                    &[
                                                        "name",
                                                        "version",
                                                        "disk",
                                                        "checksum",
                                                        "size_bytes",
                                                    ],
                                                    &format!(
                                                        "[[vms.instances]] #{inst_idx}.managed_image"
                                                    ),
                                                    &mut warnings,
                                                );
                                            } else {
                                                warnings.push(format!(
                                                    "`managed_image` on [[vms.instances]] entry #{inst_idx} must be a table."
                                                ));
                                            }
                                        }

                                        if let Some(port_forwards) =
                                            instance_table.get("port_forwards")
                                        {
                                            if let toml::Value::Array(tables) = port_forwards {
                                                for (pf_idx, pf) in tables.iter().enumerate() {
                                                    if let toml::Value::Table(pf_table) = pf {
                                                        warn_table(
                                                            pf_table,
                                                            &["host", "guest", "protocol"],
                                                            &format!(
                                                                "[[vms.instances.port_forwards]] #{pf_idx}"
                                                            ),
                                                            &mut warnings,
                                                        );
                                                    } else {
                                                        warnings.push(format!(
                                                            "[[vms.instances.port_forwards]] entry #{pf_idx} must be a table."
                                                        ));
                                                    }
                                                }
                                            } else {
                                                warnings.push(
                                                    "`port_forwards` under [[vms.instances]] must be an array of tables."
                                                        .to_string(),
                                                );
                                            }
                                        }
                                    } else {
                                        warnings.push(format!(
                                            "[[vms.instances]] entry #{inst_idx} must be a table."
                                        ));
                                    }
                                }
                            } else {
                                warnings
                                    .push("`instances` must be an array of tables.".to_string());
                            }
                        }
                    } else {
                        warnings.push(format!("[[vms]] entry #{idx} must be a table."));
                    }
                }
            } else {
                warnings.push("`vms` must be an array of tables.".to_string());
            }
        }

        if let Some(workflows) = table.get("workflows") {
            if let toml::Value::Table(workflows_table) = workflows {
                warn_table(workflows_table, &["init"], "[workflows]", &mut warnings);
            } else {
                warnings.push("Expected [workflows] to be a table.".to_string());
            }
        }

        if let Some(lifecycle) = table.get("lifecycle") {
            if let toml::Value::Table(lifecycle_table) = lifecycle {
                warn_table(
                    lifecycle_table,
                    &[
                        "graceful_shutdown_wait_secs",
                        "sigterm_wait_secs",
                        "sigkill_wait_secs",
                    ],
                    "[lifecycle]",
                    &mut warnings,
                );
            } else {
                warnings.push("Expected [lifecycle] to be a table.".to_string());
            }
        }

        if let Some(bootstrap) = table.get("bootstrap") {
            if let toml::Value::Table(bootstrap_table) = bootstrap {
                warn_table(
                    bootstrap_table,
                    &["mode", "handshake_timeout_secs", "remote_dir", "env"],
                    "[bootstrap]",
                    &mut warnings,
                );
                if let Some(env) = bootstrap_table.get("env") {
                    if !env.is_table() {
                        warnings.push(
                            "`[bootstrap].env` must be a table mapping environment variables."
                                .to_string(),
                        );
                    }
                }
            } else {
                warnings.push("Expected [bootstrap] to be a table.".to_string());
            }
        }
    }

    warnings
}

fn find_legacy_broker_keys(value: &toml::Value) -> Vec<String> {
    let mut keys = Vec::new();
    if let toml::Value::Table(table) = value {
        if table.contains_key("broker") {
            keys.push("[broker]".to_string());
        }
    }
    keys
}

fn warn_table(
    table: &toml::map::Map<String, toml::Value>,
    allowed: &[&str],
    context: &str,
    warnings: &mut Vec<String>,
) {
    for key in table.keys() {
        if !allowed.contains(&key.as_str()) {
            warnings.push(format!(
                "Unknown field `{key}` at {context}; this value will be ignored."
            ));
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum SchemaKind {
    Legacy,
    MultiInstance,
}

impl SchemaKind {
    fn supports_multi_instance(self) -> bool {
        matches!(self, SchemaKind::MultiInstance)
    }
}

fn classify_schema_version(raw: &str) -> Result<(SchemaKind, bool), String> {
    let (major, minor, _patch) = parse_version_components(raw).ok_or_else(|| {
        format!(
            "Configuration version `{raw}` is not a valid semantic version (expected formats like \"0.2.0\")."
        )
    })?;

    match (major, minor) {
        (0, 1) => Ok((SchemaKind::Legacy, false)),
        (0, 2) => Ok((SchemaKind::MultiInstance, false)),
        (0, m) if m > 2 => Ok((SchemaKind::MultiInstance, true)),
        (m, _) if m > 0 => Ok((SchemaKind::MultiInstance, true)),
        _ => Err(format!(
            "Configuration version `{raw}` is not supported; use at least \"0.1.0\"."
        )),
    }
}

fn parse_version_components(raw: &str) -> Option<(u64, u64, u64)> {
    let mut parts = raw.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next().unwrap_or("0").parse().ok()?;
    let patch = parts.next().unwrap_or("0").parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((major, minor, patch))
}

fn resolve_base_image(
    path: &Path,
    context: &str,
    config_root: &Path,
    state_root: &Path,
    base_image: Option<PathBuf>,
    managed_image: Option<RawManagedImage>,
) -> Result<BaseImageSource, Error> {
    if managed_image.is_some() {
        return Err(invalid_config(
            path,
            format!(
                "{context} declares `managed_image`, which is no longer supported. Remove it and set `base_image` or rely on the default Alpine image."
            ),
        ));
    }

    if let Some(path_buf) = base_image {
        return Ok(BaseImageSource::new(
            resolve_path(config_root, path_buf),
            BaseImageProvenance::Explicit,
        ));
    }

    Ok(BaseImageSource::new(
        default_alpine_base_image_path(state_root),
        BaseImageProvenance::DefaultAlpine,
    ))
}

pub fn default_alpine_base_image_path(state_root: &Path) -> PathBuf {
    state_root
        .join(DEFAULT_IMAGE_SUBDIR)
        .join(DEFAULT_ALPINE_IMAGE_FILENAME)
}

pub(crate) fn default_overlay_base_path(state_root: &Path, role_name: &str) -> PathBuf {
    let mut base = state_root.join(DEFAULT_OVERLAY_SUBDIR);
    let slug = overlay_role_slug(role_name);
    let file_name = format!(
        "{slug}-{}.{}",
        DEFAULT_OVERLAY_SUFFIX, DEFAULT_OVERLAY_EXTENSION
    );
    base.push(file_name);
    base
}

fn overlay_role_slug(role_name: &str) -> String {
    let mut slug = String::with_capacity(role_name.len());
    let mut last_was_dash = true;
    for ch in role_name.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            last_was_dash = false;
        } else if !last_was_dash {
            slug.push('-');
            last_was_dash = true;
        }
    }

    let trimmed = slug.trim_matches('-');
    let base = if trimmed.is_empty() {
        "vm".to_string()
    } else {
        trimmed.to_string()
    };

    let mut hasher = Sha256::new();
    hasher.update(role_name.as_bytes());
    let digest = hasher.finalize();
    let suffix = hex::encode(&digest[..3]);

    format!("{base}-{suffix}")
}

fn parse_port_forwards_list(
    path: &Path,
    scope: &str,
    raw_forwards: &[RawPortForward],
    warnings: &mut Vec<String>,
) -> Result<Vec<PortForward>, Error> {
    let mut forwards = Vec::with_capacity(raw_forwards.len());
    for forward in raw_forwards {
        let host = forward.host.ok_or_else(|| {
            invalid_config(
                path,
                format!(
                    "Port forward on {scope} is missing required `host` port. Example: `host = 2222`."
                ),
            )
        })?;
        if host == 0 {
            return Err(invalid_config(
                path,
                format!("Port forward on {scope} must use a host port between 1 and 65535."),
            ));
        }

        let guest = forward.guest.ok_or_else(|| {
            invalid_config(
                path,
                format!(
                    "Port forward on {scope} is missing required `guest` port. Example: `guest = 22`."
                ),
            )
        })?;
        if guest == 0 {
            return Err(invalid_config(
                path,
                format!("Port forward on {scope} must use a guest port between 1 and 65535."),
            ));
        }

        let protocol_raw = forward.protocol.clone();
        let protocol = protocol_raw
            .as_deref()
            .map(PortProtocol::from_str)
            .unwrap_or(Some(PortProtocol::Tcp))
            .ok_or_else(|| {
                invalid_config(
                    path,
                    format!(
                        "Port forward on {scope} has unsupported protocol `{}`. Supported values: `tcp`, `udp`.",
                        protocol_raw.unwrap()
                    ),
                )
            })?;

        forwards.push(PortForward {
            host,
            guest,
            protocol,
        });
    }

    let mut guest_port_usage: HashMap<(u16, PortProtocol), usize> = HashMap::new();
    for forward in &forwards {
        let counter = guest_port_usage
            .entry((forward.guest, forward.protocol))
            .or_default();
        *counter += 1;
    }
    for ((guest_port, protocol), count) in guest_port_usage {
        if count > 1 {
            warnings.push(format!(
                "{scope} declares {count} forwards for guest port {guest_port}/{protocol}; consider consolidating."
            ));
        }
    }

    Ok(forwards)
}

fn parse_replica_index(role_name: &str, id: &str) -> Result<usize, String> {
    let prefix = format!("{role_name}-");
    if !id.starts_with(&prefix) {
        return Err(format!(
            "Replica override id `{id}` must match `<{role_name}>-<index>`."
        ));
    }

    let suffix = &id[prefix.len()..];
    if suffix.is_empty() {
        return Err(format!(
            "Replica override id `{id}` is missing the numeric `<index>` portion."
        ));
    }

    let index: usize = suffix
        .parse()
        .map_err(|_| format!("Replica override id `{id}` contains a non-numeric index."))?;

    let canonical = format!("{role_name}-{index}");
    if canonical != id {
        return Err(format!(
            "Replica override id `{id}` must not include leading zeros; expected `{canonical}`."
        ));
    }

    Ok(index)
}

fn derive_overlay_for_instance(
    base: &Path,
    index: usize,
    total: usize,
    multi_enabled: bool,
) -> PathBuf {
    if !multi_enabled || total <= 1 || index == 0 {
        return base.to_path_buf();
    }

    if let Some(file_name) = base.file_name() {
        if let Some(name) = file_name.to_str() {
            let (stem, ext) = split_file_name(name);
            let new_file = if let Some(ext) = ext {
                format!("{stem}-{index}.{ext}")
            } else {
                format!("{stem}-{index}")
            };
            return base.with_file_name(new_file);
        }

        let mut os_name = file_name.to_os_string();
        os_name.push(format!("-{index}"));
        return base.with_file_name(PathBuf::from(os_name));
    }

    base.join(format!("{index}"))
}

fn split_file_name(name: &str) -> (&str, Option<&str>) {
    if let Some((stem, ext)) = name.rsplit_once('.') {
        if !stem.is_empty() {
            return (stem, Some(ext));
        }
    }
    (name, None)
}

#[derive(Debug, Deserialize)]
struct RawConfig {
    version: Option<String>,
    project: Option<RawProject>,
    #[serde(default)]
    vms: Vec<RawVm>,
    #[serde(default)]
    workflows: RawWorkflows,
    #[serde(default)]
    lifecycle: Option<RawLifecycle>,
    #[serde(default)]
    bootstrap: Option<RawBootstrap>,
}

#[derive(Debug, Deserialize)]
struct RawProject {
    name: Option<String>,
    #[serde(default)]
    state_dir: Option<PathBuf>,
    #[serde(default)]
    features: RawProjectFeatures,
}

#[derive(Debug, Deserialize, Default)]
struct RawProjectFeatures {
    #[serde(default)]
    enable_vm_vizier: Option<bool>,
}

impl RawProjectFeatures {
    fn into_features(self, warnings: &mut Vec<String>) -> ProjectFeatures {
        if self.enable_vm_vizier.is_some() {
            warnings.push(
                "[project.features] enable_vm_vizier is deprecated and ignored; in-VM Vizier is always enabled."
                    .to_string(),
            );
        }
        ProjectFeatures
    }
}

#[derive(Debug, Deserialize)]
struct RawVm {
    name: Option<String>,
    #[serde(default)]
    description: Option<String>,
    base_image: Option<PathBuf>,
    #[serde(default)]
    managed_image: Option<RawManagedImage>,
    overlay: Option<PathBuf>,
    #[serde(default)]
    cpus: Option<u32>,
    #[serde(default)]
    memory: Option<String>,
    #[serde(default, rename = "port_forwards")]
    port_forwards: Vec<RawPortForward>,
    #[serde(default)]
    count: Option<u32>,
    #[serde(default)]
    instances: Vec<RawVmInstance>,
    #[serde(default)]
    bootstrap: Option<RawVmBootstrap>,
}

#[derive(Debug, Deserialize)]
struct RawPortForward {
    host: Option<u16>,
    guest: Option<u16>,
    #[serde(default)]
    protocol: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawVmInstance {
    id: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    base_image: Option<PathBuf>,
    #[serde(default)]
    managed_image: Option<RawManagedImage>,
    #[serde(default)]
    overlay: Option<PathBuf>,
    #[serde(default)]
    cpus: Option<u32>,
    #[serde(default)]
    memory: Option<String>,
    #[serde(default, rename = "port_forwards")]
    port_forwards: Option<Vec<RawPortForward>>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct RawManagedImage {
    name: Option<String>,
    version: Option<String>,
    #[serde(default)]
    disk: Option<String>,
    #[serde(default)]
    checksum: Option<String>,
    #[serde(default)]
    size_bytes: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct RawBootstrap {
    #[serde(default)]
    mode: Option<String>,
    #[serde(default)]
    handshake_timeout_secs: Option<u64>,
    #[serde(default)]
    remote_dir: Option<PathBuf>,
    #[serde(default)]
    env: HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct RawVmBootstrap {
    #[serde(default)]
    mode: Option<String>,
    #[serde(default)]
    script: Option<PathBuf>,
    #[serde(default)]
    payload: Option<PathBuf>,
    #[serde(default)]
    handshake_timeout_secs: Option<u64>,
    #[serde(default)]
    remote_dir: Option<PathBuf>,
    #[serde(default)]
    env: HashMap<String, String>,
    #[serde(default)]
    verify_command: Option<String>,
    #[serde(default)]
    verify_path: Option<PathBuf>,
}

#[derive(Debug)]
struct InstanceOverride {
    id: String,
    data: InstanceOverrideData,
}

#[derive(Debug)]
struct InstanceOverrideData {
    description: Option<String>,
    base_image: Option<PathBuf>,
    managed_image: Option<RawManagedImage>,
    overlay: Option<PathBuf>,
    cpus: Option<u32>,
    memory: Option<String>,
    port_forwards: Option<Vec<RawPortForward>>,
}

#[derive(Debug, Deserialize, Default)]
struct RawWorkflows {
    #[serde(default)]
    init: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct RawLifecycle {
    #[serde(default)]
    graceful_shutdown_wait_secs: Option<u64>,
    #[serde(default)]
    sigterm_wait_secs: Option<u64>,
    #[serde(default)]
    sigkill_wait_secs: Option<u64>,
}

impl RawLifecycle {
    fn into_config(self, path: &Path) -> Result<LifecycleConfig, Error> {
        let graceful = self
            .graceful_shutdown_wait_secs
            .unwrap_or(DEFAULT_GRACEFUL_SHUTDOWN_WAIT_SECS);
        let sigterm = self.sigterm_wait_secs.unwrap_or(DEFAULT_SIGTERM_WAIT_SECS);
        let sigkill = self.sigkill_wait_secs.unwrap_or(DEFAULT_SIGKILL_WAIT_SECS);

        if sigkill == 0 {
            return Err(invalid_config(
                path,
                "`[lifecycle].sigkill_wait_secs` must be at least 1 to allow the orchestrator to confirm process exit.",
            ));
        }

        Ok(LifecycleConfig {
            graceful_shutdown_wait_secs: graceful,
            sigterm_wait_secs: sigterm,
            sigkill_wait_secs: sigkill,
        })
    }
}

impl RawBootstrap {
    fn into_config(self, path: &Path) -> Result<BootstrapConfig, Error> {
        let mode = match self.mode.as_deref() {
            Some(value) => value
                .parse::<BootstrapMode>()
                .map_err(|err| invalid_config(path, err))?,
            None => BootstrapMode::default(),
        };

        let handshake_timeout_secs = match self.handshake_timeout_secs {
            Some(0) => {
                return Err(invalid_config(
                    path,
                    "`[bootstrap].handshake_timeout_secs` must be at least 1 second.",
                ));
            }
            Some(value) => value,
            None => DEFAULT_BOOTSTRAP_HANDSHAKE_WAIT_SECS,
        };

        let remote_dir = match self.remote_dir {
            Some(dir) if dir.as_os_str().is_empty() => {
                return Err(invalid_config(
                    path,
                    "`[bootstrap].remote_dir` must not be empty.",
                ));
            }
            Some(dir) => dir,
            None => PathBuf::from("/tmp/castra-bootstrap"),
        };

        Ok(BootstrapConfig {
            mode,
            handshake_timeout_secs,
            remote_dir,
            env: self.env,
        })
    }
}

impl RawVmBootstrap {
    fn resolve_mode(&self, path: &Path, context: &str) -> Result<Option<BootstrapMode>, Error> {
        match self.mode.as_deref() {
            Some(value) => value
                .parse::<BootstrapMode>()
                .map_err(|_| {
                    invalid_config(
                        path,
                        format!(
                            "{context} has unknown bootstrap.mode `{value}`. Supported values: auto, skip, always."
                        ),
                    )
                })
                .map(Some),
            None => Ok(None),
        }
    }
}

impl RawConfig {
    fn into_validated(
        self,
        path: &Path,
        warnings: &mut Vec<String>,
    ) -> Result<ProjectConfig, Error> {
        let RawConfig {
            version,
            project,
            vms,
            workflows,
            lifecycle,
            bootstrap: bootstrap_raw,
        } = self;

        let version = version.ok_or_else(|| {
            invalid_config(
                path,
                "Missing required top-level field `version`. Example: `version = \"0.1.0\"`.",
            )
        })?;

        let (schema, warn_future) =
            classify_schema_version(&version).map_err(|message| invalid_config(path, message))?;
        if warn_future {
            warnings.push(format!(
                "Configuration version `{version}` is not fully supported yet; proceeding anyway."
            ));
        }

        let raw_project = project.ok_or_else(|| {
            invalid_config(
                path,
                "Missing required table `[project]`. Example:\n\
                 [project]\n\
                 name = \"devbox\"",
            )
        })?;

        let project_name = raw_project.name.ok_or_else(|| {
            invalid_config(
                path,
                "Missing required field `project.name`. Example: `name = \"devbox\"`.",
            )
        })?;

        let features = raw_project.features.into_features(warnings);

        let project_root = path.parent().map(Path::to_path_buf).unwrap_or_else(|| {
            warnings.push(
                "Unable to determine config directory; assuming current working directory."
                    .to_string(),
            );
            PathBuf::from(".")
        });

        let state_root = raw_project
            .state_dir
            .map(|dir| resolve_path(&project_root, dir))
            .unwrap_or_else(|| default_state_root(&project_name, path));

        let bootstrap_config = match bootstrap_raw {
            Some(raw) => raw.into_config(path)?,
            None => BootstrapConfig::default(),
        };

        if vms.is_empty() {
            return Err(invalid_config(
                path,
                "At least one `[[vms]]` entry is required. Example:\n\
                 [[vms]]\n\
                 name = \"devbox\"\n\
                 base_image = \"images/devbox.qcow2\"\n\
                 overlay = \".castra/devbox-overlay.qcow2\"\n\
                 (Tip: run `castra init` to scaffold a starter config.)",
            ));
        }

        let root_dir = project_root.clone();
        let supports_multi = schema.supports_multi_instance();

        let mut seen_roles = HashSet::new();
        let mut seen_instances = HashSet::new();
        let mut expanded_vms = Vec::new();

        for vm in vms {
            let RawVm {
                name,
                description,
                base_image: raw_base_image,
                managed_image: raw_managed_image,
                overlay: raw_overlay,
                cpus,
                memory,
                port_forwards,
                count,
                instances,
                bootstrap,
            } = vm;

            let role_name = name.ok_or_else(|| {
                invalid_config(
                    path,
                    "Each `[[vms]]` entry must define `name`. Example: `name = \"devbox\"`.",
                )
            })?;

            if !seen_roles.insert(role_name.clone()) {
                return Err(invalid_config(
                    path,
                    format!(
                        "Duplicate VM role `{role_name}` detected. Each role must have a unique `name`."
                    ),
                ));
            }

            if !supports_multi && (count.is_some() || !instances.is_empty()) {
                return Err(invalid_config(
                    path,
                    format!(
                        "VM `{role_name}` uses replica fields but config version `{version}` only supports single-instance roles. \
                         Remove `count`/`[[vms.instances]]` or set `version = \"0.2.0\"`."
                    ),
                ));
            }

            let count_value = count.unwrap_or(1);
            if count_value == 0 {
                return Err(invalid_config(
                    path,
                    format!("VM `{role_name}` declares `count = 0`. Specify at least one replica."),
                ));
            }
            if !supports_multi && count_value != 1 {
                return Err(invalid_config(
                    path,
                    format!(
                        "VM `{role_name}` declares `count = {count_value}` but config version `{version}` only supports single-instance roles."
                    ),
                ));
            }
            let count_usize = count_value as usize;

            let context = format!("VM `{role_name}`");
            let base_image = resolve_base_image(
                path,
                &context,
                &root_dir,
                &state_root,
                raw_base_image,
                raw_managed_image,
            )?;

            let base_overlay = match raw_overlay {
                Some(overlay) => resolve_overlay_path(&root_dir, &state_root, overlay),
                None => default_overlay_base_path(&state_root, &role_name),
            };

            let base_cpus = cpus.unwrap_or(2);
            if base_cpus == 0 {
                return Err(invalid_config(
                    path,
                    format!("VM `{role_name}` must request at least one CPU. Example: `cpus = 2`."),
                ));
            }

            let memory_string = memory.unwrap_or_else(|| "2048 MiB".to_string());
            let base_memory = parse_memory(&memory_string).map_err(|msg| {
                invalid_config(
                    path,
                    format!(
                        "VM `{role_name}` has invalid memory specification `{memory_string}`: {msg}. \
                         Example values: `2048 MiB`, `2 GiB`."
                    ),
                )
            })?;

            let base_description = description;

            let base_forwards = parse_port_forwards_list(
                path,
                &format!("VM `{role_name}`"),
                &port_forwards,
                warnings,
            )?;

            let base_bootstrap_mode = match bootstrap.as_ref() {
                Some(config) => config
                    .resolve_mode(path, &format!("VM `{role_name}`"))?
                    .unwrap_or(bootstrap_config.mode),
                None => bootstrap_config.mode,
            };

            let mut overrides = HashMap::new();
            if supports_multi {
                for raw_override in instances {
                    let RawVmInstance {
                        id,
                        description,
                        base_image,
                        managed_image,
                        overlay,
                        cpus,
                        memory,
                        port_forwards,
                    } = raw_override;

                    let id = id.ok_or_else(|| {
                        invalid_config(
                            path,
                            format!(
                                "Replica override under VM `{role_name}` is missing `id`. Expected `id = \"{role_name}-0\"`, etc."
                            ),
                        )
                    })?;

                    let index = parse_replica_index(&role_name, &id)
                        .map_err(|msg| invalid_config(path, msg))?;

                    if index >= count_usize {
                        return Err(invalid_config(
                            path,
                            format!(
                                "Replica override `{id}` exceeds declared `count = {count_value}` for VM `{role_name}`."
                            ),
                        ));
                    }

                    if overrides
                        .insert(
                            index,
                            InstanceOverride {
                                id,
                                data: InstanceOverrideData {
                                    description,
                                    base_image,
                                    managed_image,
                                    overlay,
                                    cpus,
                                    memory,
                                    port_forwards,
                                },
                            },
                        )
                        .is_some()
                    {
                        return Err(invalid_config(
                            path,
                            format!(
                                "Replica override `{role_name}-{index}` declared multiple times. Each replica can only appear once."
                            ),
                        ));
                    }
                }
            }

            for idx in 0..count_usize {
                let instance_name = if supports_multi {
                    format!("{role_name}-{idx}")
                } else {
                    role_name.clone()
                };

                if !seen_instances.insert(instance_name.clone()) {
                    return Err(invalid_config(
                        path,
                        format!("Duplicate VM name `{instance_name}` detected after expansion."),
                    ));
                }

                let (description, image_source, cpus, memory_spec, overlay_path, forwards) =
                    if let Some(InstanceOverride { id, data }) = overrides.remove(&idx) {
                        let InstanceOverrideData {
                            description,
                            base_image: override_base_image,
                            managed_image: override_managed_image,
                            overlay: override_overlay,
                            cpus: override_cpus,
                            memory: override_memory,
                            port_forwards: override_forwards,
                        } = data;

                        let image_source =
                            if override_base_image.is_none() && override_managed_image.is_none() {
                                base_image.clone()
                            } else {
                                resolve_base_image(
                                    path,
                                    &format!("Replica `{id}`"),
                                    &root_dir,
                                    &state_root,
                                    override_base_image,
                                    override_managed_image,
                                )?
                            };

                        let overlay_path = override_overlay
                            .map(|overlay| resolve_overlay_path(&root_dir, &state_root, overlay))
                            .unwrap_or_else(|| {
                                derive_overlay_for_instance(
                                    &base_overlay,
                                    idx,
                                    count_usize,
                                    supports_multi,
                                )
                            });

                        let cpus = override_cpus.unwrap_or(base_cpus);
                        if cpus == 0 {
                            return Err(invalid_config(
                                path,
                                format!("Replica `{id}` must request at least one CPU."),
                            ));
                        }

                        let memory_spec = match override_memory {
                            Some(value) => parse_memory(&value).map_err(|msg| {
                                invalid_config(
                                    path,
                                    format!(
                                        "Replica `{id}` has invalid memory specification `{value}`: {msg}. \
                                         Example values: `2048 MiB`, `2 GiB`."
                                    ),
                                )
                            })?,
                            None => base_memory.clone(),
                        };

                        let description = description.or_else(|| base_description.clone());

                        let forwards = match override_forwards {
                            Some(raw) => parse_port_forwards_list(
                                path,
                                &format!("Replica `{id}`"),
                                &raw,
                                warnings,
                            )?,
                            None => base_forwards.clone(),
                        };

                        (
                            description,
                            image_source,
                            cpus,
                            memory_spec,
                            overlay_path,
                            forwards,
                        )
                    } else {
                        (
                            base_description.clone(),
                            base_image.clone(),
                            base_cpus,
                            base_memory.clone(),
                            derive_overlay_for_instance(
                                &base_overlay,
                                idx,
                                count_usize,
                                supports_multi,
                            ),
                            base_forwards.clone(),
                        )
                    };

                let bootstrap_override = bootstrap.as_ref();

                if let Some(cfg) = bootstrap_override {
                    if let Some(script_path) = cfg.script.as_ref() {
                        if script_path.as_os_str().is_empty() {
                            return Err(invalid_config(
                                path,
                                format!(
                                    "VM `{instance_name}` declares an empty `bootstrap.script`; specify a script path or omit the field."
                                ),
                            ));
                        }
                    }
                    if let Some(payload_path) = cfg.payload.as_ref() {
                        if payload_path.as_os_str().is_empty() {
                            return Err(invalid_config(
                                path,
                                format!(
                                    "VM `{instance_name}` declares an empty `bootstrap.payload`; specify a payload directory or omit the field."
                                ),
                            ));
                        }
                    }
                }

                let script_override = bootstrap_override
                    .and_then(|cfg| cfg.script.as_ref())
                    .cloned()
                    .map(|path| resolve_path(&project_root, path));
                let default_script_path = project_root
                    .join("bootstrap")
                    .join(&instance_name)
                    .join("run.sh");
                let script_path = script_override.unwrap_or(default_script_path);

                let payload_override = bootstrap_override
                    .and_then(|cfg| cfg.payload.as_ref())
                    .cloned()
                    .map(|path| resolve_path(&project_root, path));
                let default_payload_path = project_root
                    .join("bootstrap")
                    .join(&instance_name)
                    .join("payload");
                let payload_path = payload_override.unwrap_or(default_payload_path);

                let handshake_timeout_secs = match bootstrap_override
                    .and_then(|cfg| cfg.handshake_timeout_secs)
                {
                    Some(0) => {
                        return Err(invalid_config(
                            path,
                            format!(
                                "VM `{instance_name}` declares `bootstrap.handshake_timeout_secs = 0`; specify at least 1 second.",
                            ),
                        ));
                    }
                    Some(value) => value,
                    None => bootstrap_config.handshake_timeout_secs,
                };

                let remote_dir = match bootstrap_override.and_then(|cfg| cfg.remote_dir.as_ref()) {
                    Some(dir) if dir.as_os_str().is_empty() => {
                        return Err(invalid_config(
                            path,
                            format!(
                                "VM `{instance_name}` declares an empty `bootstrap.remote_dir`; specify a non-empty path.",
                            ),
                        ));
                    }
                    Some(dir) => dir.clone(),
                    None => bootstrap_config.remote_dir.clone(),
                };

                let mut env = bootstrap_config.env.clone();
                if let Some(cfg) = bootstrap_override {
                    for (key, value) in &cfg.env {
                        env.insert(key.clone(), value.clone());
                    }
                }

                let verify = bootstrap_override.and_then(|cfg| {
                    if cfg.verify_command.is_some() || cfg.verify_path.is_some() {
                        Some(BootstrapVerifyConfig {
                            command: cfg.verify_command.clone(),
                            path: cfg.verify_path.clone(),
                        })
                    } else {
                        None
                    }
                });

                expanded_vms.push(VmDefinition {
                    name: instance_name,
                    role_name: role_name.clone(),
                    replica_index: idx,
                    description,
                    base_image: image_source,
                    overlay: overlay_path,
                    cpus,
                    memory: memory_spec,
                    port_forwards: forwards,
                    bootstrap: VmBootstrapConfig {
                        mode: base_bootstrap_mode,
                        script: Some(script_path),
                        payload: Some(payload_path),
                        handshake_timeout_secs,
                        remote_dir,
                        env,
                        verify,
                    },
                });
            }

            if let Some(extra) = overrides.into_values().next() {
                return Err(invalid_config(
                    path,
                    format!(
                        "Replica override `{}` was not applied. Verify the ID matches `<{role_name}>-<index>` within the declared count.",
                        extra.id
                    ),
                ));
            }
        }

        let workflows = Workflows {
            init: workflows.init,
        };

        let lifecycle = match lifecycle {
            Some(raw) => raw.into_config(path)?,
            None => LifecycleConfig::default(),
        };

        Ok(ProjectConfig {
            file_path: path.to_path_buf(),
            project_root,
            version,
            project_name,
            features,
            vms: expanded_vms,
            state_root,
            workflows,
            lifecycle,
            bootstrap: bootstrap_config,
            warnings: warnings.clone(),
        })
    }
}

fn resolve_path(base: &Path, input: PathBuf) -> PathBuf {
    if input.is_absolute() {
        input
    } else if base.is_absolute() {
        base.join(input)
    } else {
        match env::current_dir() {
            Ok(cwd) => cwd.join(base).join(input),
            Err(_) => base.join(input),
        }
    }
}

fn resolve_overlay_path(config_root: &Path, state_root: &Path, overlay: PathBuf) -> PathBuf {
    if overlay.is_absolute() {
        overlay
    } else if overlay.starts_with(".castra") {
        match overlay.strip_prefix(".castra") {
            Ok(remainder) if remainder.as_os_str().is_empty() => state_root.to_path_buf(),
            Ok(remainder) => state_root.join(remainder),
            Err(_) => state_root.to_path_buf(),
        }
    } else {
        resolve_path(config_root, overlay)
    }
}

fn parse_memory(input: &str) -> Result<MemorySpec, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("memory value cannot be empty".to_string());
    }

    let mut parts = trimmed.split_whitespace();
    let amount = parts
        .next()
        .ok_or_else(|| "memory value is missing numeric component".to_string())?;
    let unit = parts.next();
    if parts.next().is_some() {
        return Err("memory value contains unexpected extra tokens".to_string());
    }

    let amount_value: f64 = amount.parse().map_err(|_| {
        format!("could not parse `{amount}` as a number; try values like `2048 MiB` or `2 GiB`")
    })?;

    let bytes = match unit.map(|u| u.to_ascii_lowercase()) {
        Some(ref u) if u == "mib" || u == "mb" => (amount_value * 1024.0 * 1024.0) as u64,
        Some(ref u) if u == "gib" || u == "gb" => (amount_value * 1024.0 * 1024.0 * 1024.0) as u64,
        Some(ref u) if u == "kib" || u == "kb" => (amount_value * 1024.0) as u64,
        Some(ref u) if u == "b" || u == "bytes" => amount_value as u64,
        Some(ref u) => {
            return Err(format!(
                "unsupported memory unit `{u}`; supported units are B, KiB, MiB, GiB."
            ));
        }
        None => (amount_value * 1024.0 * 1024.0) as u64,
    };

    Ok(MemorySpec {
        original: trimmed.to_string(),
        bytes: Some(bytes),
    })
}

pub(crate) fn default_state_root(project_name: &str, config_path: &Path) -> PathBuf {
    let home = user_home_dir().unwrap_or_else(|| PathBuf::from("."));
    let projects_root = home.join(".castra").join("projects");
    let slug = slugify_project_name(project_name);
    let unique = derive_project_id(config_path);
    projects_root.join(format!("{slug}-{unique}"))
}

fn slugify_project_name(name: &str) -> String {
    let mut slug = String::with_capacity(name.len());
    let mut last_was_dash = true;
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            last_was_dash = false;
        } else if !last_was_dash {
            slug.push('-');
            last_was_dash = true;
        }
    }

    let trimmed = slug.trim_matches('-');
    if trimmed.is_empty() {
        "project".to_string()
    } else {
        trimmed.to_string()
    }
}

fn derive_project_id(config_path: &Path) -> String {
    let parent = config_path.parent().unwrap_or_else(|| Path::new("."));
    let repr = parent.to_string_lossy();
    let mut hasher = Sha256::new();
    hasher.update(repr.as_bytes());
    let digest = hasher.finalize();
    hex::encode(&digest[..8])
}

pub(crate) fn user_home_dir() -> Option<PathBuf> {
    if let Some(home) = std::env::var_os("HOME") {
        if !home.is_empty() {
            return Some(PathBuf::from(home));
        }
    }

    #[cfg(windows)]
    {
        if let Some(profile) = std::env::var_os("USERPROFILE") {
            if !profile.is_empty() {
                return Some(PathBuf::from(profile));
            }
        }

        let drive = std::env::var_os("HOMEDRIVE");
        let path = std::env::var_os("HOMEPATH");
        if let (Some(drive), Some(path)) = (drive, path) {
            if !drive.is_empty() && !path.is_empty() {
                let mut combined = PathBuf::from(drive);
                combined.push(path);
                if combined.components().next().is_some() {
                    return Some(combined);
                }
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::Error;
    use regex::Regex;
    use std::path::{Path, PathBuf};
    use std::time::Duration;
    use temp_env::with_var;
    use tempfile::tempdir;

    fn write_config(dir: &tempfile::TempDir, contents: &str) -> PathBuf {
        let path = dir.path().join("castra.toml");
        fs::write(&path, contents).expect("write config");
        path
    }

    fn minimal_config(contents: &str) -> String {
        format!(
            r#"
version = "0.1.0"

[project]
name = "demo"
state_dir = ".castra"

{contents}
"#
        )
    }

    fn minimal_config_v02(contents: &str) -> String {
        format!(
            r#"
version = "0.2.0"

[project]
name = "demo"
state_dir = ".castra/state"

{contents}
"#
        )
    }

    #[test]
    fn load_config_with_local_base_image() {
        let dir = tempdir().unwrap();
        let path = write_config(
            &dir,
            &minimal_config(
                r#"
[[vms]]
name = "devbox"
base_image = "images/devbox.qcow2"
overlay = ".castra/devbox-overlay.qcow2"
cpus = 2
memory = "2048 MiB"
"#,
            ),
        );

        let config = load_project_config(&path).expect("load config");
        assert_eq!(config.project_name, "demo");
        assert_eq!(config.vms.len(), 1);
        let vm = &config.vms[0];
        assert_eq!(vm.name, "devbox");
        assert_eq!(vm.role_name, "devbox");
        assert_eq!(vm.replica_index, 0);
        assert_eq!(
            vm.base_image.path(),
            dir.path().join("images/devbox.qcow2").as_path(),
            "base image path is resolved relative to file"
        );
        assert_eq!(
            vm.base_image.provenance(),
            BaseImageProvenance::Explicit,
            "explicit base image marked as explicit provenance"
        );
        assert_eq!(
            vm.overlay,
            dir.path().join(".castra/devbox-overlay.qcow2"),
            "overlay resolves under explicit state_dir"
        );
        assert_eq!(vm.cpus, 2);
        assert_eq!(vm.memory.original(), "2048 MiB");
        assert_eq!(vm.memory.bytes(), Some(2048 * 1024 * 1024));
        assert!(config.warnings.is_empty());

        let conflicts = config.port_conflicts();
        assert!(conflicts.is_empty());
        assert_eq!(
            config.state_root,
            dir.path().join(".castra"),
            "state root uses project override"
        );
    }

    #[test]
    fn load_config_defaults_base_image_and_overlay() {
        let dir = tempdir().unwrap();
        let path = write_config(
            &dir,
            &minimal_config_v02(
                r#"
[[vms]]
name = "devbox"
cpus = 2
memory = "1024 MiB"
"#,
            ),
        );

        let config = load_project_config(&path).expect("load config with defaults");
        assert_eq!(config.vms.len(), 1);
        let vm = &config.vms[0];
        let expected_path = default_alpine_base_image_path(&config.state_root);
        assert_eq!(
            vm.base_image.path(),
            expected_path.as_path(),
            "default base image points at cached alpine image"
        );
        assert_eq!(
            vm.base_image.provenance(),
            BaseImageProvenance::DefaultAlpine,
            "default base image provenance recorded"
        );

        assert_eq!(
            vm.overlay,
            default_overlay_base_path(&config.state_root, "devbox")
        );
        let overlay_name = vm
            .overlay
            .file_name()
            .and_then(|name| name.to_str())
            .expect("overlay file name");
        let re = Regex::new(r"^devbox-[0-9a-f]{6}-overlay\.qcow2$").unwrap();
        assert!(
            re.is_match(overlay_name),
            "unexpected overlay file name {overlay_name}"
        );
    }

    #[test]
    fn load_config_defaults_multi_instance_overlays() {
        let dir = tempdir().unwrap();
        let path = write_config(
            &dir,
            &minimal_config_v02(
                r#"
[[vms]]
name = "Web App"
cpus = 2
memory = "1024 MiB"
count = 2
"#,
            ),
        );

        let config = load_project_config(&path).expect("load multi-instance config with defaults");
        assert_eq!(config.vms.len(), 2);
        let base_overlay = default_overlay_base_path(&config.state_root, "Web App");
        for (idx, vm) in config.vms.iter().enumerate() {
            assert_eq!(vm.replica_index, idx);
            let expected_path = default_alpine_base_image_path(&config.state_root);
            assert_eq!(vm.base_image.path(), expected_path.as_path());
            assert_eq!(
                vm.base_image.provenance(),
                BaseImageProvenance::DefaultAlpine
            );
            let expected_overlay = derive_overlay_for_instance(&base_overlay, idx, 2, true);
            assert_eq!(vm.overlay, expected_overlay);
        }
    }

    #[test]
    fn overlay_role_slug_sanitizes_names() {
        let slug = overlay_role_slug("API Box!");
        let re = Regex::new(r"^api-box-[0-9a-f]{6}$").unwrap();
        assert!(re.is_match(&slug), "unexpected slug {slug}");

        let fallback = overlay_role_slug("???");
        let fallback_re = Regex::new(r"^vm-[0-9a-f]{6}$").unwrap();
        assert!(
            fallback_re.is_match(&fallback),
            "unexpected fallback {fallback}"
        );
    }

    #[test]
    fn bootstrap_mode_from_str_supports_aliases() {
        assert_eq!(
            "auto".parse::<BootstrapMode>().unwrap(),
            BootstrapMode::Auto
        );
        assert_eq!(
            "Automatic".parse::<BootstrapMode>().unwrap(),
            BootstrapMode::Auto
        );
        assert_eq!(
            "enabled".parse::<BootstrapMode>().unwrap(),
            BootstrapMode::Auto
        );
        assert_eq!(
            "skip".parse::<BootstrapMode>().unwrap(),
            BootstrapMode::Skip
        );
        assert_eq!("off".parse::<BootstrapMode>().unwrap(), BootstrapMode::Skip);
        assert_eq!(
            "disabled".parse::<BootstrapMode>().unwrap(),
            BootstrapMode::Skip
        );
        assert_eq!(
            "always".parse::<BootstrapMode>().unwrap(),
            BootstrapMode::Always
        );
        assert_eq!(
            "force".parse::<BootstrapMode>().unwrap(),
            BootstrapMode::Always
        );
        assert!("bogus".parse::<BootstrapMode>().is_err());
    }

    #[test]
    fn parse_bootstrap_defaults_and_overrides() {
        let dir = tempdir().unwrap();
        let config_path = write_config(
            &dir,
            r#"
version = "0.2.0"

[project]
name = "demo"

[bootstrap]
mode = "always"
handshake_timeout_secs = 120
remote_dir = "/opt/bootstrap"
[bootstrap.env]
SHARED = "true"
FOO = "top"

[[vms]]
name = "dev"
base_image = "images/dev.qcow2"
overlay = ".castra/dev.qcow2"

  [vms.bootstrap]
  mode = "skip"
  script = "bootstrap/dev/run.sh"
  payload = "bootstrap/dev/payload"
  handshake_timeout_secs = 15
  remote_dir = "/opt/dev"
  verify_command = "check"
  verify_path = "/status/ok"

    [vms.bootstrap.env]
    LOCAL = "yes"
    FOO = "vm"
"#,
        );

        let project =
            load_project_config(&config_path).expect("configuration with bootstrap overrides");

        assert_eq!(project.bootstrap.mode, BootstrapMode::Always);
        assert_eq!(project.bootstrap.handshake_timeout_secs, 120);
        assert_eq!(
            project.bootstrap.remote_dir,
            PathBuf::from("/opt/bootstrap")
        );
        assert_eq!(
            project.bootstrap.env.get("SHARED"),
            Some(&"true".to_string())
        );
        assert_eq!(project.bootstrap.env.get("FOO"), Some(&"top".to_string()));

        assert_eq!(project.vms.len(), 1);
        let vm = &project.vms[0];
        assert_eq!(vm.bootstrap.mode, BootstrapMode::Skip);
        let project_root = config_path.parent().unwrap();
        assert_eq!(
            vm.bootstrap.script.as_ref().expect("script path").as_path(),
            &project_root.join("bootstrap/dev/run.sh")
        );
        assert_eq!(
            vm.bootstrap
                .payload
                .as_ref()
                .expect("payload path")
                .as_path(),
            &project_root.join("bootstrap/dev/payload")
        );
        assert_eq!(vm.bootstrap.handshake_timeout_secs, 15);
        assert_eq!(vm.bootstrap.remote_dir, PathBuf::from("/opt/dev"));
        assert_eq!(vm.bootstrap.env.get("SHARED"), Some(&"true".to_string()));
        assert_eq!(vm.bootstrap.env.get("LOCAL"), Some(&"yes".to_string()));
        assert_eq!(vm.bootstrap.env.get("FOO"), Some(&"vm".to_string()));
        let verify = vm.bootstrap.verify.as_ref().expect("verify config");
        assert_eq!(verify.command.as_deref(), Some("check"));
        assert_eq!(
            verify.path.as_ref().map(PathBuf::as_path),
            Some(Path::new("/status/ok"))
        );
    }

    #[test]
    fn lifecycle_defaults_applied() {
        let dir = tempdir().unwrap();
        let path = write_config(
            &dir,
            &minimal_config(
                r#"
[[vms]]
name = "devbox"
base_image = "images/devbox.qcow2"
overlay = ".castra/devbox-overlay.qcow2"
cpus = 2
memory = "2048 MiB"
"#,
            ),
        );

        let config = load_project_config(&path).expect("load config");
        assert_eq!(
            config.lifecycle.graceful_shutdown_wait_secs,
            DEFAULT_GRACEFUL_SHUTDOWN_WAIT_SECS
        );
        assert_eq!(
            config.lifecycle.sigterm_wait_secs,
            DEFAULT_SIGTERM_WAIT_SECS
        );
        assert_eq!(
            config.lifecycle.sigkill_wait_secs,
            DEFAULT_SIGKILL_WAIT_SECS
        );
    }

    #[test]
    fn lifecycle_overrides_parse() {
        let dir = tempdir().unwrap();
        let path = write_config(
            &dir,
            &minimal_config(
                r#"
[lifecycle]
graceful_shutdown_wait_secs = 5
sigterm_wait_secs = 2
sigkill_wait_secs = 7

[[vms]]
name = "devbox"
base_image = "images/devbox.qcow2"
overlay = ".castra/devbox-overlay.qcow2"
cpus = 2
memory = "2048 MiB"
"#,
            ),
        );

        let config = load_project_config(&path).expect("load config");
        assert_eq!(config.lifecycle.graceful_shutdown_wait_secs, 5);
        assert_eq!(config.lifecycle.sigterm_wait_secs, 2);
        assert_eq!(config.lifecycle.sigkill_wait_secs, 7);
        assert_eq!(config.lifecycle.graceful_wait(), Duration::from_secs(5));
        assert_eq!(config.lifecycle.sigterm_wait(), Duration::from_secs(2));
        assert_eq!(config.lifecycle.sigkill_wait(), Duration::from_secs(7));
    }

    #[test]
    fn lifecycle_sigkill_requires_positive_duration() {
        let dir = tempdir().unwrap();
        let path = write_config(
            &dir,
            &minimal_config(
                r#"
[lifecycle]
sigkill_wait_secs = 0

[[vms]]
name = "devbox"
base_image = "images/devbox.qcow2"
overlay = ".castra/devbox-overlay.qcow2"
cpus = 2
memory = "2048 MiB"
"#,
            ),
        );

        let err = load_project_config(&path).unwrap_err();
        match err {
            Error::InvalidConfig { message, .. } => {
                assert!(
                    message.contains("sigkill_wait_secs"),
                    "unexpected message: {message}"
                );
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn load_config_expands_multi_instance_role() {
        let dir = tempdir().unwrap();
        let path = write_config(
            &dir,
            &minimal_config_v02(
                r#"
[[vms]]
name = "api"
base_image = "images/api-base.qcow2"
overlay = ".castra/api/overlay.qcow2"
cpus = 2
memory = "2048 MiB"
count = 3

  [[vms.port_forwards]]
  host = 8080
  guest = 8080
"#,
            ),
        );

        let config = load_project_config(&path).expect("load multi-instance config");
        assert_eq!(config.vms.len(), 3);
        let expected = ["api-0", "api-1", "api-2"];
        let state_root = dir.path().join(".castra/state");
        assert_eq!(config.state_root, state_root);

        for (idx, vm) in config.vms.iter().enumerate() {
            assert_eq!(vm.name, expected[idx]);
            assert_eq!(vm.role_name, "api");
            assert_eq!(vm.replica_index, idx);
            assert_eq!(
                vm.base_image.path(),
                dir.path().join("images/api-base.qcow2").as_path()
            );
            assert_eq!(vm.base_image.provenance(), BaseImageProvenance::Explicit);
            let expected_overlay = match idx {
                0 => state_root.join("api/overlay.qcow2"),
                1 => state_root.join("api/overlay-1.qcow2"),
                2 => state_root.join("api/overlay-2.qcow2"),
                _ => unreachable!(),
            };
            assert_eq!(vm.overlay, expected_overlay);
            assert_eq!(vm.cpus, 2);
            assert_eq!(vm.port_forwards.len(), 1);
            assert_eq!(vm.port_forwards[0].host, 8080);
        }
    }

    #[test]
    fn load_config_applies_instance_overrides() {
        let dir = tempdir().unwrap();
        let path = write_config(
            &dir,
            &minimal_config_v02(
                r#"
[[vms]]
name = "api"
base_image = "images/api-base.qcow2"
overlay = ".castra/api/overlay.qcow2"
cpus = 2
memory = "2048 MiB"
count = 2

  [[vms.port_forwards]]
  host = 8080
  guest = 8080

  [[vms.instances]]
  id = "api-1"
  cpus = 4
  memory = "4096 MiB"
  overlay = ".castra/api/custom-1.qcow2"
  base_image = "images/api-custom.qcow2"

    [[vms.instances.port_forwards]]
    host = 9000
    guest = 9000
    protocol = "tcp"
"#,
            ),
        );

        let config = load_project_config(&path).expect("load overrides config");
        assert_eq!(config.vms.len(), 2);
        let state_root = dir.path().join(".castra/state");

        let primary = &config.vms[0];
        assert_eq!(primary.name, "api-0");
        assert_eq!(primary.overlay, state_root.join("api/overlay.qcow2"));
        assert_eq!(primary.cpus, 2);
        assert_eq!(primary.memory.original(), "2048 MiB");
        assert_eq!(primary.port_forwards.len(), 1);
        assert_eq!(primary.port_forwards[0].host, 8080);

        let replica = &config.vms[1];
        assert_eq!(replica.name, "api-1");
        assert_eq!(replica.cpus, 4);
        assert_eq!(replica.memory.original(), "4096 MiB");
        assert_eq!(replica.overlay, state_root.join("api/custom-1.qcow2"));
        assert_eq!(replica.port_forwards.len(), 1);
        assert_eq!(replica.port_forwards[0].host, 9000);

        assert_eq!(
            replica.base_image.path(),
            dir.path().join("images/api-custom.qcow2").as_path()
        );
        assert_eq!(
            replica.base_image.provenance(),
            BaseImageProvenance::Explicit
        );
    }

    #[test]
    fn load_config_rejects_override_with_mismatched_id() {
        let dir = tempdir().unwrap();
        let path = write_config(
            &dir,
            &minimal_config_v02(
                r#"
[[vms]]
name = "api"
base_image = "images/api-base.qcow2"
overlay = ".castra/api/overlay.qcow2"
cpus = 2
memory = "2048 MiB"
count = 1

  [[vms.instances]]
  id = "api-01"
  cpus = 4
"#,
            ),
        );

        let err = load_project_config(&path).expect_err("override id must match pattern");
        match err {
            Error::InvalidConfig { message, .. } => {
                assert!(
                    message.contains("leading zeros"),
                    "unexpected message: {message}"
                );
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn load_config_rejects_override_out_of_range() {
        let dir = tempdir().unwrap();
        let path = write_config(
            &dir,
            &minimal_config_v02(
                r#"
[[vms]]
name = "api"
base_image = "images/api-base.qcow2"
overlay = ".castra/api/overlay.qcow2"
cpus = 2
memory = "2048 MiB"
count = 1

  [[vms.instances]]
  id = "api-1"
  cpus = 4
"#,
            ),
        );

        let err = load_project_config(&path).expect_err("index must be in range");
        match err {
            Error::InvalidConfig { message, .. } => {
                assert!(
                    message.contains("exceeds declared `count"),
                    "unexpected message: {message}"
                );
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn load_config_rejects_count_zero() {
        let dir = tempdir().unwrap();
        let path = write_config(
            &dir,
            &minimal_config_v02(
                r#"
[[vms]]
name = "api"
base_image = "images/api-base.qcow2"
overlay = ".castra/api/overlay.qcow2"
cpus = 2
memory = "2048 MiB"
count = 0
"#,
            ),
        );

        let err = load_project_config(&path).expect_err("count must be positive");
        match err {
            Error::InvalidConfig { message, .. } => {
                assert!(
                    message.contains("count = 0"),
                    "unexpected message: {message}"
                );
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn load_config_rejects_replicas_on_legacy_schema() {
        let dir = tempdir().unwrap();
        let path = write_config(
            &dir,
            &minimal_config(
                r#"
[[vms]]
name = "api"
base_image = "images/api-base.qcow2"
overlay = ".castra/api/overlay.qcow2"
cpus = 2
memory = "2048 MiB"
count = 2
"#,
            ),
        );

        let err = load_project_config(&path).expect_err("legacy schema should reject count");
        match err {
            Error::InvalidConfig { message, .. } => {
                assert!(
                    message.contains("only supports single-instance"),
                    "unexpected message: {message}"
                );
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn load_config_warns_on_future_version() {
        let dir = tempdir().unwrap();
        let path = write_config(
            &dir,
            &format!(
                r#"
version = "0.3.0"

[project]
name = "demo"
state_dir = ".castra/state"

[[vms]]
name = "devbox"
base_image = "images/devbox.qcow2"
overlay = ".castra/devbox.qcow2"
cpus = 2
memory = "2048 MiB"
"#
            ),
        );

        let config = load_project_config(&path).expect("load future config");
        assert_eq!(config.vms.len(), 1);
        assert!(!config.warnings.is_empty());
        assert!(config.warnings[0].contains("not fully supported"));
    }

    #[test]
    fn load_config_rejects_duplicate_vm_names() {
        let dir = tempdir().unwrap();
        let path = write_config(
            &dir,
            &minimal_config(
                r#"
[[vms]]
name = "devbox"
base_image = "images/a.qcow2"
overlay = ".castra/a.qcow2"
cpus = 1
memory = "1 GiB"

[[vms]]
name = "devbox"
base_image = "images/b.qcow2"
overlay = ".castra/b.qcow2"
cpus = 1
memory = "1 GiB"
"#,
            ),
        );

        let err = load_project_config(&path).expect_err("should reject duplicate names");
        match err {
            Error::InvalidConfig { message, .. } => {
                assert!(
                    message.contains("Duplicate VM role"),
                    "unexpected message: {message}"
                );
            }
            _ => panic!("unexpected error variant: {err:?}"),
        }
    }

    #[test]
    fn load_config_defaults_missing_base_and_managed() {
        let dir = tempdir().unwrap();
        let path = write_config(
            &dir,
            &minimal_config(
                r#"
[[vms]]
name = "devbox"
overlay = ".castra/a.qcow2"
cpus = 1
memory = "1 GiB"
"#,
            ),
        );

        let config = load_project_config(&path).expect("defaults missing image");
        let vm = &config.vms[0];
        let expected_path = default_alpine_base_image_path(&config.state_root);
        assert_eq!(vm.base_image.path(), expected_path.as_path());
        assert_eq!(
            vm.base_image.provenance(),
            BaseImageProvenance::DefaultAlpine
        );
        assert_eq!(
            vm.overlay,
            dir.path().join(".castra/a.qcow2"),
            "explicit overlay should be preserved"
        );
    }

    #[test]
    fn load_config_rejects_managed_image_key() {
        let dir = tempdir().unwrap();
        let path = write_config(
            &dir,
            &minimal_config(
                r#"
[[vms]]
name = "devbox"
managed_image = { name = "alpine", version = "v1" }
overlay = ".castra/a.qcow2"
cpus = 1
memory = "1 GiB"
"#,
            ),
        );

        let err = load_project_config(&path).expect_err("should reject managed_image key");
        match err {
            Error::InvalidConfig { message, .. } => {
                assert!(
                    message.contains("managed_image"),
                    "unexpected error message: {message}"
                );
            }
            other => panic!("unexpected error variant: {other:?}"),
        }
    }

    #[test]
    fn parse_memory_variants() {
        let valid = [
            ("2048", 2048 * 1024 * 1024),
            ("1024 MiB", 1024 * 1024 * 1024),
            ("1 GiB", 1024 * 1024 * 1024),
            ("256 KiB", 256 * 1024),
            ("64 B", 64),
        ];
        for (input, expected) in valid {
            let spec = parse_memory(input).expect("parse memory");
            assert_eq!(spec.original(), input.trim());
            assert_eq!(spec.bytes(), Some(expected));
        }

        let err = parse_memory("").expect_err("empty string invalid");
        assert!(err.contains("cannot be empty"));
        let err = parse_memory("foo").expect_err("non-numeric invalid");
        assert!(err.contains("could not parse"));
        let err = parse_memory("10 XB").expect_err("unknown unit");
        assert!(err.contains("unsupported memory unit"));
    }

    #[test]
    fn resolve_overlay_path_handles_relative_and_dotcastra() {
        let config_root = Path::new("/tmp/project");
        let state_root = Path::new("/state/root");
        assert_eq!(
            resolve_overlay_path(config_root, state_root, PathBuf::from("/absolute/path")),
            PathBuf::from("/absolute/path")
        );
        assert_eq!(
            resolve_overlay_path(
                config_root,
                state_root,
                PathBuf::from(".castra/devbox.qcow2")
            ),
            state_root.join("devbox.qcow2")
        );
        assert_eq!(
            resolve_overlay_path(config_root, state_root, PathBuf::from(".castra")),
            state_root
        );
        assert_eq!(
            resolve_overlay_path(config_root, state_root, PathBuf::from("relative.qcow2")),
            config_root.join("relative.qcow2")
        );
    }

    #[test]
    fn resolve_path_anchors_relative_base_to_cwd() {
        let cwd = std::env::current_dir().expect("current dir");
        let base = Path::new("examples/bootstrap-quickstart");
        let target = PathBuf::from("../minimal-bootstrap/alpine-x86_64.qcow2");
        let resolved = super::resolve_path(base, target.clone());
        assert_eq!(resolved, cwd.join(base).join(target));
    }

    #[test]
    fn slugify_project_name_strips_symbols() {
        assert_eq!(slugify_project_name("Dev Box!"), "dev-box");
        assert_eq!(slugify_project_name("  "), "project");
        assert_eq!(slugify_project_name("Rust_CPU"), "rust-cpu");
    }

    #[test]
    fn derive_project_id_changes_with_path() {
        let dir_a = tempdir().unwrap();
        let dir_b = tempdir().unwrap();
        let config_a = dir_a.path().join("castra.toml");
        let config_b = dir_b.path().join("castra.toml");
        let id_a = derive_project_id(&config_a);
        let id_b = derive_project_id(&config_b);
        assert_ne!(
            id_a, id_b,
            "different directories should yield different ids"
        );
        let re = Regex::new("^[a-f0-9]{16}$").unwrap();
        assert!(re.is_match(&id_a));
    }

    #[test]
    fn default_state_root_uses_home_directory() {
        let home = tempdir().unwrap();
        with_var("HOME", Some(home.path().to_str().unwrap()), || {
            let config = home.path().join("project").join("castra.toml");
            let state = default_state_root("My Project", &config);
            assert!(state.starts_with(home.path().join(".castra/projects")));
        });
    }

    #[test]
    fn user_home_dir_respects_env() {
        let home = tempdir().unwrap();
        with_var("HOME", Some(home.path().to_str().unwrap()), || {
            let detected = user_home_dir().expect("home should be detected");
            assert_eq!(detected, home.path());
        });
    }

    #[test]
    fn detect_unknown_fields_reports_extra_keys() {
        let value: toml::Value = toml::from_str(
            r#"
version = "0.1.0"
unexpected = 1
[project]
name = "demo"
extra = true
"#,
        )
        .unwrap();
        let warnings = detect_unknown_fields(&value);
        assert!(
            warnings
                .iter()
                .any(|w| w.contains("Unknown field `unexpected` at root")),
            "warnings missing root notice: {warnings:?}"
        );
        assert!(
            warnings
                .iter()
                .any(|w| w.contains("Unknown field `extra` at [project]")),
            "warnings missing project notice: {warnings:?}"
        );
    }
}
