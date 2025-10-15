use std::path::PathBuf;
use std::time::SystemTime;

use crate::managed::{
    ManagedArtifactEventDetail, ManagedArtifactKind, ManagedImageArtifactExpectation,
    ManagedImageArtifactSummary, ManagedImageProfileOutcome, ManagedImageSpec,
    ManagedImageVerificationOutcome,
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
    /// Managed image verification started and expectations recorded.
    ManagedImageVerificationStarted {
        /// The managed image specification undergoing verification.
        spec: ManagedImageSpecHandle,
        /// When verification was initiated.
        started_at: SystemTime,
        /// Expected artifacts (filenames, hashes) planned for verification.
        plan: Vec<ManagedImageArtifactExpectation>,
    },
    /// Outcome of managed image verification including artifacts.
    ManagedImageVerificationResult {
        /// The managed image specification that was verified.
        spec: ManagedImageSpecHandle,
        /// When verification completed.
        completed_at: SystemTime,
        /// Milliseconds spent verifying artifacts.
        duration_ms: u64,
        /// Outcome of the verification.
        outcome: ManagedImageVerificationOutcome,
        /// Artifact summaries including filenames, sizes, and checksums.
        artifacts: Vec<ManagedImageArtifactSummary>,
    },
    /// Structured details about a boot profile being applied to a VM.
    ManagedImageProfileApplied {
        /// The managed image specification providing the profile.
        spec: ManagedImageSpecHandle,
        /// VM name receiving the profile.
        vm: String,
        /// When profile application started.
        started_at: SystemTime,
        /// Components that will be applied to the VM boot configuration.
        components: ManagedImageProfileComponents,
    },
    /// Result of applying the managed image profile to a VM.
    ManagedImageProfileResult {
        /// The managed image specification providing the profile.
        spec: ManagedImageSpecHandle,
        /// VM name receiving the profile.
        vm: String,
        /// When profile application completed.
        completed_at: SystemTime,
        /// Milliseconds spent preparing the profile application.
        duration_ms: u64,
        /// Outcome of the profile application.
        outcome: ManagedImageProfileOutcome,
        /// Components that were applied to the VM boot configuration.
        components: ManagedImageProfileComponents,
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
    CooperativeAttempted {
        /// Name of the VM.
        vm: String,
        /// Cooperative channel used for the attempt.
        method: CooperativeMethod,
        /// Milliseconds the host will wait before escalating.
        timeout_ms: u64,
    },
    /// Guest acknowledged and completed the cooperative shutdown.
    CooperativeSucceeded {
        /// Name of the VM.
        vm: String,
        /// Milliseconds elapsed before the VM exited.
        elapsed_ms: u64,
    },
    /// Guest failed to exit within the cooperative window.
    CooperativeTimedOut {
        /// Name of the VM.
        vm: String,
        /// Milliseconds waited for the cooperative shutdown.
        waited_ms: u64,
        /// Structured reason explaining why the cooperative phase concluded.
        reason: CooperativeTimeoutReason,
        /// Optional detail string for diagnostics (e.g. socket errors).
        detail: Option<String>,
    },
    /// Host escalated shutdown beyond cooperative attempts.
    ShutdownEscalated {
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
    /// Host-side bootstrap pipeline started for a VM.
    BootstrapStarted {
        /// Name of the VM.
        vm: String,
        /// Hash identifying the base image used for the VM.
        base_hash: String,
        /// Hash representing the bootstrap artifact contents.
        artifact_hash: String,
        /// Trigger that requested the bootstrap run (auto vs always).
        trigger: BootstrapTrigger,
    },
    /// Progress update for a specific bootstrap step.
    BootstrapStep {
        /// Name of the VM.
        vm: String,
        /// Step within the bootstrap pipeline being reported.
        step: BootstrapStepKind,
        /// Outcome of the step execution.
        status: BootstrapStepStatus,
        /// Milliseconds spent in the step.
        duration_ms: u64,
        /// Optional human-readable detail about the step outcome.
        detail: Option<String>,
    },
    /// Host-side bootstrap pipeline completed successfully or determined it was unnecessary.
    BootstrapCompleted {
        /// Name of the VM.
        vm: String,
        /// Completion status (success vs noop).
        status: BootstrapStatus,
        /// Milliseconds spent across the bootstrap run.
        duration_ms: u64,
        /// Optional stamp identifier recorded under the state root.
        stamp: Option<String>,
    },
    /// Host-side bootstrap pipeline failed.
    BootstrapFailed {
        /// Name of the VM.
        vm: String,
        /// Milliseconds spent before the failure.
        duration_ms: u64,
        /// Error message describing the failure cause.
        error: String,
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
        /// Evidence linking reclaimed bytes to managed image verification results, when available.
        managed_evidence: Vec<CleanupManagedImageEvidence>,
    },
}

/// Components that comprise a managed image boot profile.
#[derive(Debug, Clone)]
pub struct ManagedImageProfileComponents {
    /// Resolved kernel path on disk.
    pub kernel: PathBuf,
    /// Optional initrd path.
    pub initrd: Option<PathBuf>,
    /// Kernel append/cmdline used.
    pub append: String,
    /// Additional QEMU arguments supplied by the profile.
    pub extra_args: Vec<String>,
    /// Machine type override if provided.
    pub machine: Option<String>,
}

/// Evidence linking cleanup actions to prior managed image verification events.
#[derive(Debug, Clone)]
pub struct CleanupManagedImageEvidence {
    /// Managed image identifier.
    pub image_id: String,
    /// Managed image version associated with the verification entry.
    pub image_version: String,
    /// Path to the log containing the verification record.
    pub log_path: PathBuf,
    /// Timestamp (UTC seconds since epoch) when verification completed.
    pub verified_at: SystemTime,
    /// Artifact filenames recorded in the verification result.
    pub artifacts: Vec<String>,
}

/// Trigger that initiated a bootstrap run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootstrapTrigger {
    /// Run initiated in automatic mode after detecting changes.
    Auto,
    /// Run forced regardless of previous stamp state.
    Always,
}

/// Kind of step recorded during bootstrap execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootstrapStepKind {
    /// Waiting for a fresh broker handshake to confirm guest reachability.
    WaitHandshake,
    /// Establishing SSH connectivity with the guest.
    Connect,
    /// Transferring artifacts/scripts to the guest.
    Transfer,
    /// Executing the guest bootstrap script.
    Apply,
    /// Verifying the bootstrap outcome (remote stamp / host stamp persistence).
    Verify,
}

/// Result of executing a bootstrap step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootstrapStepStatus {
    /// Step succeeded as expected.
    Success,
    /// Step was skipped because no work was required.
    Skipped,
    /// Step failed; details provided alongside the event.
    Failed,
}

/// Final bootstrap completion disposition for a VM.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootstrapStatus {
    /// Bootstrap executed successfully.
    Success,
    /// Bootstrap determined no work was required (stamp already satisfied).
    NoOp,
}

/// Cooperative channel used during guest shutdown attempts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CooperativeMethod {
    /// ACPI-triggered shutdown via QMP `system_powerdown`.
    Acpi,
    /// Guest agent issuing an orderly shutdown.
    Agent,
    /// No cooperative channel was available.
    Unavailable,
}

impl CooperativeMethod {
    /// Human-friendly label for rendering.
    pub fn describe(self) -> &'static str {
        match self {
            CooperativeMethod::Acpi => "ACPI (QMP system_powerdown)",
            CooperativeMethod::Agent => "guest agent channel",
            CooperativeMethod::Unavailable => "no cooperative channel",
        }
    }
}

/// Why the cooperative phase concluded without confirmation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CooperativeTimeoutReason {
    /// The VM remained running until the timeout expired.
    TimeoutExpired,
    /// No cooperative channel was available.
    ChannelUnavailable,
    /// Attempt failed due to I/O or protocol error.
    ChannelError,
}

impl CooperativeTimeoutReason {
    /// Human-friendly label for rendering.
    pub fn describe(self) -> &'static str {
        match self {
            CooperativeTimeoutReason::TimeoutExpired => "timeout expired",
            CooperativeTimeoutReason::ChannelUnavailable => "channel unavailable",
            CooperativeTimeoutReason::ChannelError => "channel error",
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
