use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::error::CliError;

pub const DEFAULT_BROKER_PORT: u16 = 7070;

#[derive(Debug, Clone)]
pub enum BaseImageSource {
    Path(PathBuf),
    Managed(ManagedImageReference),
}

impl BaseImageSource {
    pub fn describe(&self) -> String {
        match self {
            BaseImageSource::Path(path) => path.display().to_string(),
            BaseImageSource::Managed(reference) => format!(
                "managed:{}@{} ({})",
                reference.name,
                reference.version,
                reference.disk.describe()
            ),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ManagedImageReference {
    pub name: String,
    pub version: String,
    pub disk: ManagedDiskKind,
}

#[derive(Debug, Clone)]
pub enum ManagedDiskKind {
    RootDisk,
}

impl ManagedDiskKind {
    pub fn parse(input: Option<String>) -> Result<Self, String> {
        match input {
            None => Ok(Self::RootDisk),
            Some(value) => match value.as_str() {
                "root" | "rootfs" | "root_disk" => Ok(Self::RootDisk),
                other => Err(format!(
                    "Unknown managed disk kind `{other}`. Supported values: root"
                )),
            },
        }
    }

    pub fn describe(&self) -> &'static str {
        match self {
            ManagedDiskKind::RootDisk => "root disk",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProjectConfig {
    pub file_path: PathBuf,
    pub version: String,
    pub project_name: String,
    pub vms: Vec<VmDefinition>,
    pub workflows: Workflows,
    pub broker: BrokerConfig,
    pub warnings: Vec<String>,
}

impl ProjectConfig {
    pub fn port_conflicts(&self) -> (Vec<PortConflict>, Option<BrokerCollision>) {
        let mut map: HashMap<u16, Vec<&VmDefinition>> = HashMap::new();
        for vm in &self.vms {
            for forward in &vm.port_forwards {
                map.entry(forward.host).or_default().push(vm);
            }
        }

        let duplicates = map
            .into_iter()
            .filter(|(_, vms)| vms.len() > 1)
            .map(|(port, vms)| PortConflict {
                port,
                vm_names: vms.iter().map(|vm| vm.name.clone()).collect(),
            })
            .collect();

        let broker_collision = if self.vms.iter().any(|vm| {
            vm.port_forwards
                .iter()
                .any(|pf| pf.host == self.broker.port)
        }) {
            Some(BrokerCollision {
                port: self.broker.port,
            })
        } else {
            None
        };

        (duplicates, broker_collision)
    }
}

#[derive(Debug, Clone)]
pub struct VmDefinition {
    pub name: String,
    pub description: Option<String>,
    pub base_image: BaseImageSource,
    pub overlay: PathBuf,
    pub cpus: u32,
    pub memory: MemorySpec,
    pub port_forwards: Vec<PortForward>,
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
pub struct BrokerConfig {
    pub port: u16,
}

#[derive(Debug, Clone)]
pub struct PortConflict {
    pub port: u16,
    pub vm_names: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct BrokerCollision {
    pub port: u16,
}

pub fn load_project_config(path: &Path) -> Result<ProjectConfig, CliError> {
    let contents = fs::read_to_string(path).map_err(|source| CliError::ReadConfig {
        path: path.to_path_buf(),
        source,
    })?;

    let value: toml::Value = toml::from_str(&contents).map_err(|source| CliError::ParseConfig {
        path: path.to_path_buf(),
        source,
    })?;

    let mut warnings = detect_unknown_fields(&value);

    let raw = RawConfig::deserialize(value).map_err(|source| CliError::ParseConfig {
        path: path.to_path_buf(),
        source,
    })?;

    raw.into_validated(path, &mut warnings)
}

fn invalid_config(path: &Path, message: impl Into<String>) -> CliError {
    CliError::InvalidConfig {
        path: path.to_path_buf(),
        message: message.into(),
    }
}

fn detect_unknown_fields(value: &toml::Value) -> Vec<String> {
    let mut warnings = Vec::new();
    let allowed_root = ["version", "project", "vms", "workflows", "broker"];

    if let toml::Value::Table(table) = value {
        warn_table(table, &allowed_root, "root", &mut warnings);

        if let Some(project) = table.get("project") {
            if let toml::Value::Table(project_table) = project {
                warn_table(project_table, &["name"], "[project]", &mut warnings);
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
                            ],
                            &format!("[[vms]] #{idx}"),
                            &mut warnings,
                        );

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

        if let Some(broker) = table.get("broker") {
            if let toml::Value::Table(broker_table) = broker {
                warn_table(broker_table, &["port"], "[broker]", &mut warnings);
            } else {
                warnings.push("Expected [broker] to be a table.".to_string());
            }
        }
    }

    warnings
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

#[derive(Debug, Deserialize)]
struct RawConfig {
    version: Option<String>,
    project: Option<RawProject>,
    #[serde(default)]
    vms: Vec<RawVm>,
    #[serde(default)]
    workflows: RawWorkflows,
    #[serde(default)]
    broker: Option<RawBroker>,
}

#[derive(Debug, Deserialize)]
struct RawProject {
    name: Option<String>,
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
}

#[derive(Debug, Deserialize)]
struct RawPortForward {
    host: Option<u16>,
    guest: Option<u16>,
    #[serde(default)]
    protocol: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawManagedImage {
    name: Option<String>,
    version: Option<String>,
    #[serde(default)]
    disk: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct RawWorkflows {
    #[serde(default)]
    init: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct RawBroker {
    port: Option<u16>,
}

impl RawConfig {
    fn into_validated(
        self,
        path: &Path,
        warnings: &mut Vec<String>,
    ) -> Result<ProjectConfig, CliError> {
        let version = self.version.ok_or_else(|| {
            invalid_config(
                path,
                "Missing required top-level field `version`. Example: `version = \"0.1.0\"`.",
            )
        })?;

        if version != "0.1.0" {
            warnings.push(format!(
                "Configuration version `{version}` is not fully supported yet; proceeding anyway."
            ));
        }

        let project = self.project.ok_or_else(|| {
            invalid_config(
                path,
                "Missing required table `[project]`. Example:\n\
                 [project]\n\
                 name = \"devbox\"",
            )
        })?;

        let project_name = project.name.ok_or_else(|| {
            invalid_config(
                path,
                "Missing required field `project.name`. Example: `name = \"devbox\"`.",
            )
        })?;

        if self.vms.is_empty() {
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

        let root_dir = path.parent().map(Path::to_path_buf).unwrap_or_else(|| {
            warnings.push(
                "Unable to determine config directory; assuming current working directory."
                    .to_string(),
            );
            PathBuf::from(".")
        });

        let mut seen_vm_names = HashSet::new();
        let mut vms = Vec::with_capacity(self.vms.len());

        for vm in self.vms {
            let name = vm.name.ok_or_else(|| {
                invalid_config(
                    path,
                    "Each `[[vms]]` entry must define `name`. Example: `name = \"devbox\"`.",
                )
            })?;

            if !seen_vm_names.insert(name.clone()) {
                return Err(invalid_config(
                    path,
                    format!(
                        "Duplicate VM name `{name}` detected. Each VM must have a unique `name`."
                    ),
                ));
            }

            let base_image = match (vm.base_image, vm.managed_image) {
                (Some(path), None) => BaseImageSource::Path(resolve_path(&root_dir, path)),
                (None, Some(managed)) => {
                    let name = managed.name.ok_or_else(|| {
                        invalid_config(
                            path,
                            format!(
                                "VM `{name}` declares `managed_image` but is missing required field `name`."
                            ),
                        )
                    })?;
                    let version = managed.version.ok_or_else(|| {
                        invalid_config(
                            path,
                            format!(
                                "VM `{name}` declares `managed_image` but is missing required field `version`."
                            ),
                        )
                    })?;
                    let disk = ManagedDiskKind::parse(managed.disk).map_err(|message| {
                        invalid_config(
                            path,
                            format!(
                                "VM `{name}` declares `managed_image` with invalid `disk`: {message}"
                            ),
                        )
                    })?;
                    BaseImageSource::Managed(ManagedImageReference {
                        name,
                        version,
                        disk,
                    })
                }
                (Some(_), Some(_)) => {
                    return Err(invalid_config(
                        path,
                        format!(
                            "VM `{name}` declares both `base_image` and `managed_image`. Choose one operand."
                        ),
                    ));
                }
                (None, None) => {
                    return Err(invalid_config(
                        path,
                        format!("VM `{name}` must declare either `base_image` or `managed_image`."),
                    ));
                }
            };

            let overlay = vm.overlay.ok_or_else(|| {
                invalid_config(
                    path,
                    format!(
                        "VM `{name}` is missing required field `overlay`. Example: `overlay = \".castra/{name}-overlay.qcow2\"`."
                    ),
                )
            })?;

            let cpus = vm.cpus.unwrap_or(2);
            if cpus == 0 {
                return Err(invalid_config(
                    path,
                    format!("VM `{name}` must request at least one CPU. Example: `cpus = 2`."),
                ));
            }

            let memory = vm.memory.unwrap_or_else(|| "2048 MiB".to_string());
            let memory_spec = parse_memory(&memory).map_err(|msg| {
                invalid_config(
                    path,
                    format!(
                        "VM `{name}` has invalid memory specification `{memory}`: {msg}. \
                         Example values: `2048 MiB`, `2 GiB`."
                    ),
                )
            })?;

            let mut forwards = Vec::with_capacity(vm.port_forwards.len());
            for forward in vm.port_forwards {
                let host = forward.host.ok_or_else(|| {
                    invalid_config(
                        path,
                        format!(
                            "Port forward on VM `{name}` is missing required `host` port. Example: `host = 2222`."
                        ),
                    )
                })?;
                if host == 0 {
                    return Err(invalid_config(
                        path,
                        format!(
                            "Port forward on VM `{name}` must use a host port between 1 and 65535."
                        ),
                    ));
                }

                let guest = forward.guest.ok_or_else(|| {
                    invalid_config(
                        path,
                        format!(
                            "Port forward on VM `{name}` is missing required `guest` port. Example: `guest = 22`."
                        ),
                    )
                })?;
                if guest == 0 {
                    return Err(invalid_config(
                        path,
                        format!(
                            "Port forward on VM `{name}` must use a guest port between 1 and 65535."
                        ),
                    ));
                }

                let protocol = forward
                    .protocol
                    .as_deref()
                    .map(PortProtocol::from_str)
                    .unwrap_or(Some(PortProtocol::Tcp))
                    .ok_or_else(|| {
                        invalid_config(
                            path,
                            format!(
                                "Port forward on VM `{name}` has unsupported protocol `{}`. Supported values: `tcp`, `udp`.",
                                forward.protocol.unwrap()
                            ),
                        )
                    })?;

                forwards.push(PortForward {
                    host,
                    guest,
                    protocol,
                });
            }

            // Check duplicate guest ports per VM to help debugging.
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
                        "VM `{name}` declares {count} forwards for guest port {guest_port}/{protocol}; consider consolidating."
                    ));
                }
            }

            vms.push(VmDefinition {
                name,
                description: vm.description,
                base_image,
                overlay: resolve_path(&root_dir, overlay),
                cpus,
                memory: memory_spec,
                port_forwards: forwards,
            });
        }

        let workflows = Workflows {
            init: self.workflows.init,
        };

        let broker = BrokerConfig {
            port: self
                .broker
                .and_then(|b| b.port)
                .unwrap_or(DEFAULT_BROKER_PORT),
        };

        Ok(ProjectConfig {
            file_path: path.to_path_buf(),
            version,
            project_name,
            vms,
            workflows,
            broker,
            warnings: warnings.clone(),
        })
    }
}

fn resolve_path(base: &Path, input: PathBuf) -> PathBuf {
    if input.is_absolute() {
        input
    } else {
        base.join(input)
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
