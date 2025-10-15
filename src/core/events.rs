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
    /// Ordered lifecycle events for VM shutdown.
    ShutdownRequested {
        /// Name of the VM.
        vm: String,
    },
    /// A guest-cooperative shutdown channel was attempted.
    GuestCooperativeAttempted {
        /// Name of the VM.
        vm: String,
        /// Cooperative channel used for the attempt.
        method: GuestCooperativeMethod,
        /// Milliseconds the host will wait before escalating.
        timeout_ms: u64,
    },
    /// Guest acknowledged and completed the cooperative shutdown.
    GuestCooperativeConfirmed {
        /// Name of the VM.
        vm: String,
        /// Milliseconds elapsed before the VM exited.
        elapsed_ms: u64,
    },
    /// Guest failed to exit within the cooperative window.
    GuestCooperativeTimeout {
        /// Name of the VM.
        vm: String,
        /// Milliseconds waited for the cooperative shutdown.
        waited_ms: u64,
        /// Structured reason explaining why the cooperative phase concluded.
        reason: GuestCooperativeTimeoutReason,
        /// Optional detail string for diagnostics (e.g. socket errors).
        detail: Option<String>,
    },
    /// Host-side termination began (signals or verification).
    HostTerminate {
        /// Name of the VM.
        vm: String,
        /// Signal sent to the host process, when applicable.
        signal: Option<ShutdownSignal>,
        /// Milliseconds the host will wait for the process to exit.
        timeout_ms: Option<u64>,
    },
    /// Host forced termination through SIGKILL or equivalent.
    HostKill {
        /// Name of the VM.
        vm: String,
        /// Signal that was sent to the process.
        signal: ShutdownSignal,
        /// Optional wait the host will observe after issuing the kill.
        timeout_ms: Option<u64>,
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

/// Cooperative channel used during guest shutdown attempts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuestCooperativeMethod {
    /// QMP `system_powerdown`, typically routed through ACPI.
    QmpSystemPowerdown,
    /// No cooperative channel was available.
    Unavailable,
}

impl GuestCooperativeMethod {
    /// Human-friendly label for rendering.
    pub fn describe(self) -> &'static str {
        match self {
            GuestCooperativeMethod::QmpSystemPowerdown => "QMP system_powerdown (ACPI)",
            GuestCooperativeMethod::Unavailable => "no cooperative channel",
        }
    }
}

/// Why the cooperative phase concluded without confirmation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuestCooperativeTimeoutReason {
    /// The VM remained running until the timeout expired.
    TimeoutExpired,
    /// No cooperative channel was available.
    ChannelUnavailable,
    /// Attempt failed due to I/O or protocol error.
    ChannelError,
}

impl GuestCooperativeTimeoutReason {
    /// Human-friendly label for rendering.
    pub fn describe(self) -> &'static str {
        match self {
            GuestCooperativeTimeoutReason::TimeoutExpired => "timeout expired",
            GuestCooperativeTimeoutReason::ChannelUnavailable => "channel unavailable",
            GuestCooperativeTimeoutReason::ChannelError => "channel error",
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
