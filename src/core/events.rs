use std::path::PathBuf;

use crate::managed::{
    ManagedArtifactEventDetail, ManagedArtifactKind, ManagedImageArtifactSummary, ManagedImageSpec,
};

use super::diagnostics::Severity;

/// Structured event emitted during long-running operations.
#[derive(Debug, Clone)]
pub enum Event {
    /// A textual progress update with a severity level.
    Message {
        /// Severity of the message.
        severity: Severity,
        /// Human-readable text.
        text: String,
    },
    /// Structured summary confirming managed image verification.
    ManagedImageVerified {
        /// The managed image specification that was verified.
        spec: ManagedImageSpecHandle,
        /// Artifact summaries including filenames, sizes, and checksums.
        artifacts: Vec<ManagedImageArtifactSummary>,
    },
    /// Structured details about a boot profile applied to a VM.
    ManagedImageProfileApplied {
        /// The managed image specification providing the profile.
        spec: ManagedImageSpecHandle,
        /// VM name receiving the profile.
        vm: String,
        /// Resolved kernel path on disk.
        kernel: PathBuf,
        /// Optional initrd path.
        initrd: Option<PathBuf>,
        /// Kernel append/cmdline used.
        append: String,
        /// Additional QEMU arguments supplied by the profile.
        extra_args: Vec<String>,
        /// Machine type override if provided.
        machine: Option<String>,
    },
    /// Status update for managed artifact acquisition.
    ManagedArtifact {
        /// The artifact specification that is being provisioned.
        spec: ManagedImageSpecHandle,
        /// Which managed artifact the event refers to (root disk, kernel, ...).
        artifact: ManagedArtifactKind,
        /// Structured detail describing the progress step.
        detail: ManagedArtifactEventDetail,
        /// Human-readable progress message.
        text: String,
    },
    /// Notification that a VM overlay image was prepared.
    OverlayPrepared {
        /// Name of the VM.
        vm: String,
        /// Filesystem path to the overlay.
        overlay_path: PathBuf,
    },
    /// Notification that a VM process was launched.
    VmLaunched {
        /// Name of the VM.
        vm: String,
        /// Operating system process identifier.
        pid: u32,
    },
    /// Notification that a coordinated shutdown was initiated for a VM.
    ShutdownInitiated {
        /// Name of the VM.
        vm: String,
        /// Method used to initiate the shutdown sequence.
        method: ShutdownMethod,
    },
    /// Notification that the shutdown path is escalating to a stronger signal.
    ShutdownEscalation {
        /// Name of the VM.
        vm: String,
        /// Signal that was sent to the process.
        signal: ShutdownSignal,
    },
    /// Notification that a VM completed its shutdown sequence.
    ShutdownComplete {
        /// Name of the VM.
        vm: String,
        /// Outcome of the shutdown path (graceful vs forced).
        outcome: ShutdownOutcome,
        /// Whether the VM transitioned state (`true` if it was running, `false` if already stopped).
        changed: bool,
    },
    /// The broker process started listening.
    BrokerStarted {
        /// OS process identifier.
        pid: u32,
        /// Port used for the broker.
        port: u16,
    },
    /// The broker process was stopped. `changed` indicates whether any action was taken.
    BrokerStopped {
        /// Whether a change occurred (`true` if the broker was terminated, `false` if it was already offline).
        changed: bool,
    },
    /// Progress emitted during cleanup operations.
    CleanupProgress {
        /// Path targeted by the cleanup step.
        path: PathBuf,
        /// Category of artifact being processed.
        kind: CleanupKind,
        /// Number of bytes associated with the action.
        bytes: u64,
        /// Whether the action occurred in dry-run mode.
        dry_run: bool,
    },
}

/// Strategy used to initiate a shutdown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShutdownMethod {
    /// Cooperative shutdown via guest-aware channel (e.g., ACPI/QMP).
    Graceful,
    /// Signal-based shutdown when cooperative paths are unavailable.
    Signals,
}

impl ShutdownMethod {
    /// Human-friendly label for rendering.
    pub fn describe(self) -> &'static str {
        match self {
            ShutdownMethod::Graceful => "graceful (ACPI)",
            ShutdownMethod::Signals => "signals (TERM/KILL)",
        }
    }
}

/// Artifact categories that the cleanup pipeline operates on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CleanupKind {
    /// Managed image cache contents.
    ManagedImages,
    /// Orchestrator log directory.
    Logs,
    /// Broker handshake artifacts.
    Handshakes,
    /// VM overlay disks.
    Overlay,
    /// Orchestrator pid files.
    PidFile,
}

impl CleanupKind {
    /// Human-friendly label for rendering.
    pub fn describe(self) -> &'static str {
        match self {
            CleanupKind::ManagedImages => "managed-images",
            CleanupKind::Logs => "logs",
            CleanupKind::Handshakes => "handshakes",
            CleanupKind::Overlay => "overlay",
            CleanupKind::PidFile => "pid-file",
        }
    }
}

/// Signals used when escalating shutdown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShutdownSignal {
    /// POSIX SIGTERM.
    Sigterm,
    /// POSIX SIGKILL.
    Sigkill,
}

impl ShutdownSignal {
    /// Human-friendly label for rendering.
    pub fn describe(self) -> &'static str {
        match self {
            ShutdownSignal::Sigterm => "SIGTERM",
            ShutdownSignal::Sigkill => "SIGKILL",
        }
    }
}

/// Result of the shutdown sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShutdownOutcome {
    /// The VM exited cleanly after the graceful attempt.
    Graceful,
    /// The VM required signals (TERM/KILL) to exit.
    Forced,
}

impl ShutdownOutcome {
    /// Human-friendly label for rendering.
    pub fn describe(self) -> &'static str {
        match self {
            ShutdownOutcome::Graceful => "graceful",
            ShutdownOutcome::Forced => "forced",
        }
    }
}

/// Handle that identifies a managed image specification without leaking internal references.
#[derive(Debug, Clone)]
pub struct ManagedImageSpecHandle {
    /// Stable managed image identifier.
    pub id: String,
    /// Managed image version.
    pub version: String,
    /// Human-readable description of the disk kind.
    pub disk: String,
}

impl From<&'static ManagedImageSpec> for ManagedImageSpecHandle {
    fn from(spec: &'static ManagedImageSpec) -> Self {
        let disk = spec
            .artifacts
            .iter()
            .find(|artifact| matches!(artifact.kind, ManagedArtifactKind::RootDisk))
            .map(|artifact| artifact.kind.describe().to_string())
            .unwrap_or_else(|| "root disk".to_string());
        Self {
            id: spec.id.to_string(),
            version: spec.version.to_string(),
            disk,
        }
    }
}
