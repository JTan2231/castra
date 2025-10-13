use std::path::PathBuf;
use std::time::Duration;

use crate::config::{BrokerConfig, PortForward};
use crate::managed::ManagedImagePaths;

use super::diagnostics::Diagnostic;
use super::events::{CleanupKind, Event, ManagedImageSpecHandle};
use super::options::{BusLogTarget, PortsView};

/// Result wrapper returned by high-level operations.
pub type OperationResult<T> = crate::error::Result<OperationOutput<T>>;

/// Envelope for successful operation outcomes.
#[derive(Debug)]
pub struct OperationOutput<T> {
    /// Primary value produced by the operation.
    pub value: T,
    /// Diagnostics collected while performing the operation.
    pub diagnostics: Vec<Diagnostic>,
    /// Structured events captured during the run.
    pub events: Vec<Event>,
}

impl<T> OperationOutput<T> {
    /// Create a new operation output.
    pub fn new(value: T) -> Self {
        Self {
            value,
            diagnostics: Vec::new(),
            events: Vec::new(),
        }
    }

    /// Attach diagnostics to the output.
    pub fn with_diagnostics(mut self, diagnostics: Vec<Diagnostic>) -> Self {
        self.diagnostics = diagnostics;
        self
    }

    /// Attach events to the output.
    pub fn with_events(mut self, events: Vec<Event>) -> Self {
        self.events = events;
        self
    }
}

/// Outcome of `init`.
#[derive(Debug)]
pub struct InitOutcome {
    pub config_path: PathBuf,
    pub project_name: String,
    pub state_root: PathBuf,
    pub overlay_root: PathBuf,
    pub did_overwrite: bool,
}

/// Outcome of `up`.
#[derive(Debug)]
pub struct UpOutcome {
    pub state_root: PathBuf,
    pub log_root: PathBuf,
    pub launched_vms: Vec<VmLaunchOutcome>,
    pub broker: Option<BrokerLaunchOutcome>,
}

#[derive(Debug)]
pub struct VmLaunchOutcome {
    pub name: String,
    pub pid: u32,
    pub assets: ManagedVmAssets,
    pub overlay_created: bool,
}

#[derive(Debug)]
pub struct ManagedVmAssets {
    pub managed_spec: Option<ManagedImageSpecHandle>,
    pub managed_paths: Option<ManagedImagePaths>,
}

#[derive(Debug)]
pub struct BrokerLaunchOutcome {
    pub pid: u32,
    pub config: BrokerConfig,
}

/// Outcome of `down`.
#[derive(Debug)]
pub struct DownOutcome {
    pub vm_results: Vec<VmShutdownOutcome>,
    pub broker: BrokerShutdownOutcome,
}

#[derive(Debug)]
pub struct VmShutdownOutcome {
    pub name: String,
    pub changed: bool,
}

#[derive(Debug)]
pub struct BrokerShutdownOutcome {
    pub changed: bool,
}

/// Outcome of `status`.
#[derive(Debug, Clone)]
pub struct StatusOutcome {
    pub project_path: PathBuf,
    pub project_name: String,
    pub config_version: String,
    pub broker_port: u16,
    pub broker_state: BrokerState,
    pub reachable: bool,
    pub last_handshake_vm: Option<String>,
    pub last_handshake_age_ms: Option<u64>,
    pub rows: Vec<VmStatusRow>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrokerState {
    Running { pid: i32 },
    Offline,
}

#[derive(Debug, Clone)]
pub struct VmStatusRow {
    pub name: String,
    pub state: String,
    pub cpus: u32,
    pub memory: String,
    pub uptime: Option<Duration>,
    pub broker_reachability: BrokerReachability,
    pub handshake_age: Option<Duration>,
    pub forwards: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrokerReachability {
    Offline,
    Waiting,
    Reachable,
}

impl BrokerReachability {
    pub fn as_str(self) -> &'static str {
        match self {
            BrokerReachability::Offline => "offline",
            BrokerReachability::Waiting => "waiting",
            BrokerReachability::Reachable => "reachable",
        }
    }
}

/// Outcome of `ports`.
#[derive(Debug)]
pub struct PortsOutcome {
    pub project_path: PathBuf,
    pub project_name: String,
    pub config_version: String,
    pub broker_port: u16,
    pub declared: Vec<PortForwardRow>,
    pub conflicts: Vec<PortConflictRow>,
    pub vm_details: Vec<VmPortDetail>,
    pub without_forwards: Vec<String>,
    pub view: PortsView,
}

#[derive(Debug)]
pub struct PortForwardRow {
    pub vm: String,
    pub forward: PortForward,
    pub status: PortForwardStatus,
    pub inactive_reason: Option<PortInactiveReason>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortForwardStatus {
    Declared,
    Active,
    Conflicting,
    BrokerReserved,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortInactiveReason {
    VmStopped,
    PortNotBound,
    InspectionUnavailable,
}

#[derive(Debug)]
pub struct PortConflictRow {
    pub port: u16,
    pub vm_names: Vec<String>,
}

#[derive(Debug)]
pub struct VmPortDetail {
    pub name: String,
    pub description: Option<String>,
    pub base_image: String,
    pub overlay: PathBuf,
    pub cpus: u32,
    pub memory: String,
    pub memory_bytes: Option<u64>,
    pub port_forwards: Vec<PortForward>,
}

/// Outcome of `logs`.
#[derive(Debug)]
pub struct LogsOutcome {
    pub sections: Vec<LogSection>,
    pub follower: Option<LogFollower>,
}

/// Outcome of `bus publish`.
#[derive(Debug)]
pub struct BusPublishOutcome {
    pub log_path: PathBuf,
    pub topic: String,
}

/// Outcome of `bus tail`.
#[derive(Debug)]
pub struct BusTailOutcome {
    pub project_path: PathBuf,
    pub project_name: String,
    pub target: BusLogTarget,
    pub log_label: String,
    pub log_path: PathBuf,
    pub entries: Vec<LogEntry>,
    pub state: LogSectionState,
    pub follower: Option<LogFollower>,
}

#[derive(Debug)]
pub struct LogSection {
    pub label: String,
    pub path: PathBuf,
    pub entries: Vec<LogEntry>,
    pub state: LogSectionState,
}

#[derive(Debug)]
pub struct LogEntry {
    pub line: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogSectionState {
    NotCreated,
    Empty,
    HasEntries,
}

/// Handle that can be polled to follow log updates.
#[derive(Debug)]
pub struct LogFollower {
    inner: Vec<LogSourceState>,
}

#[derive(Debug)]
struct LogSourceState {
    label: String,
    path: PathBuf,
    offset: u64,
}

/// Outcome of `clean`.
#[derive(Debug)]
pub struct CleanOutcome {
    /// Whether the invocation was a dry run.
    pub dry_run: bool,
    /// Cleanup results for each processed state root.
    pub state_roots: Vec<StateRootCleanup>,
}

/// Summary for a single state root cleanup.
#[derive(Debug)]
pub struct StateRootCleanup {
    /// Filesystem path to the state root.
    pub state_root: PathBuf,
    /// Optional project name associated with the state root.
    pub project_name: Option<String>,
    /// Total bytes reclaimed (0 during dry runs).
    pub reclaimed_bytes: u64,
    /// Individual actions taken or skipped.
    pub actions: Vec<CleanupAction>,
}

/// Individual cleanup decisions for a path.
#[derive(Debug)]
pub enum CleanupAction {
    /// The path was removed successfully.
    Removed {
        /// Filesystem path that was removed.
        path: PathBuf,
        /// Number of bytes reclaimed.
        bytes: u64,
        /// Kind of artifact that was removed.
        kind: CleanupKind,
    },
    /// The path was skipped for the provided reason.
    Skipped {
        /// Filesystem path that was skipped.
        path: PathBuf,
        /// Reason for skipping the cleanup.
        reason: SkipReason,
        /// Kind of artifact associated with the path.
        kind: CleanupKind,
    },
}

/// Reason why a cleanup action was skipped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkipReason {
    /// The path did not exist.
    Missing,
    /// Dry-run mode prevented deletion.
    DryRun,
    /// The path was disabled by user flags.
    FlagDisabled,
    /// Managed-only mode suppressed the path.
    ManagedOnly,
    /// A running process prevented safe cleanup.
    RunningProcess,
    /// Input/output error prevented deletion.
    Io(String),
}

impl LogFollower {
    fn new(states: Vec<LogSourceState>) -> Self {
        Self { inner: states }
    }

    pub(crate) fn from_sources<S: Into<String>>(sources: Vec<(S, PathBuf, u64)>) -> Self {
        let states = sources
            .into_iter()
            .map(|(label, path, offset)| LogSourceState {
                label: label.into(),
                path,
                offset,
            })
            .collect();
        Self::new(states)
    }

    /// Poll log sources for new lines.
    pub fn poll(&mut self) -> crate::error::Result<Vec<(String, Option<String>)>> {
        use std::fs;
        use std::io::{BufRead, BufReader, Seek, SeekFrom};

        let mut updates = Vec::new();
        for source in &mut self.inner {
            match fs::File::open(&source.path) {
                Ok(mut file) => {
                    if source.offset > 0 {
                        if let Err(err) = file.seek(SeekFrom::Start(source.offset)) {
                            return Err(crate::error::Error::LogReadFailed {
                                path: source.path.clone(),
                                source: err,
                            });
                        }
                    }

                    let mut reader = BufReader::new(file);
                    let mut buffer = String::new();
                    loop {
                        buffer.clear();
                        let bytes = reader.read_line(&mut buffer).map_err(|err| {
                            crate::error::Error::LogReadFailed {
                                path: source.path.clone(),
                                source: err,
                            }
                        })?;
                        if bytes == 0 {
                            break;
                        }
                        source.offset += bytes as u64;
                        while buffer.ends_with('\n') || buffer.ends_with('\r') {
                            buffer.pop();
                        }
                        if buffer.is_empty() {
                            updates.push((source.label.clone(), None));
                        } else {
                            updates.push((source.label.clone(), Some(buffer.clone())));
                        }
                    }
                }
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
                Err(err) => {
                    return Err(crate::error::Error::LogReadFailed {
                        path: source.path.clone(),
                        source: err,
                    });
                }
            }
        }

        Ok(updates)
    }
}
