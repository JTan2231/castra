use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use crate::managed::{
    ManagedArtifactEventDetail, ManagedArtifactKind, ManagedImageProfileOutcome, ManagedImageSpec,
    ManagedImageVerificationOutcome,
};

use crate::config::BootstrapMode;

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
        /// Identifier of the managed image being verified.
        image_id: String,
        /// Version of the managed image.
        image_version: String,
        /// Filesystem path to the managed image root disk on disk.
        image_path: PathBuf,
        /// When verification was initiated.
        started_at: SystemTime,
        /// Expected artifacts (paths, hashes, sizes) planned for verification.
        plan: Vec<ManagedImageArtifactPlan>,
    },
    /// Outcome of managed image verification including artifacts.
    ManagedImageVerificationResult {
        /// Identifier of the managed image that was verified.
        image_id: String,
        /// Version of the managed image.
        image_version: String,
        /// Filesystem path to the managed image root disk on disk.
        image_path: PathBuf,
        /// When verification completed.
        completed_at: SystemTime,
        /// Milliseconds spent verifying artifacts.
        duration_ms: u64,
        /// Outcome of the verification.
        outcome: ManagedImageVerificationOutcome,
        /// Optional failure detail when outcome is unsuccessful.
        error: Option<String>,
        /// Total size of verified artifacts (bytes).
        size_bytes: u64,
        /// Artifact summaries including paths, sizes, and checksums.
        artifacts: Vec<ManagedImageArtifactReport>,
    },
    /// Structured details about a boot profile being applied to a VM.
    ManagedImageProfileApplied {
        /// Identifier of the managed image providing the profile.
        image_id: String,
        /// Version of the managed image providing the profile.
        image_version: String,
        /// VM name receiving the profile.
        vm: String,
        /// Identifier of the applied profile.
        profile_id: String,
        /// When profile application started.
        started_at: SystemTime,
        /// Steps that will be applied to the VM boot configuration.
        steps: Vec<String>,
    },
    /// Result of applying the managed image profile to a VM.
    ManagedImageProfileResult {
        /// Identifier of the managed image providing the profile.
        image_id: String,
        /// Version of the managed image providing the profile.
        image_version: String,
        /// VM name receiving the profile.
        vm: String,
        /// Identifier of the applied profile.
        profile_id: String,
        /// When profile application completed.
        completed_at: SystemTime,
        /// Milliseconds spent preparing the profile application.
        duration_ms: u64,
        /// Outcome of the profile application.
        outcome: ManagedImageProfileOutcome,
        /// Optional failure detail when outcome is unsuccessful.
        error: Option<String>,
        /// Steps that were applied to the VM boot configuration.
        steps: Vec<String>,
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
    /// Ephemeral overlay data was discarded for a VM.
    EphemeralLayerDiscarded {
        /// Name of the VM.
        vm: String,
        /// Filesystem path to the overlay that was removed.
        overlay_path: PathBuf,
        /// Number of bytes reclaimed by deleting the overlay.
        reclaimed_bytes: u64,
        /// Why the cleanup was performed (normal shutdown vs orphan recovery).
        reason: EphemeralCleanupReason,
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
        /// Milliseconds elapsed from shutdown request until completion.
        total_ms: u64,
        /// Whether the VM transitioned state (`true` if it was running, `false` if already stopped).
        changed: bool,
    },
    /// Host-side bootstrap pipeline started for a VM.
    BootstrapPlanned {
        /// Name of the VM.
        vm: String,
        /// Effective bootstrap mode.
        mode: BootstrapMode,
        /// Whether the pipeline would run, skip, or error.
        action: BootstrapPlanAction,
        /// Short explanation describing the decision.
        reason: String,
        /// Trigger the run would use when applicable.
        trigger: Option<BootstrapTrigger>,
        /// Resolved bootstrap script path if available on disk.
        script_path: Option<PathBuf>,
        /// Resolved payload directory when present.
        payload_path: Option<PathBuf>,
        /// Total payload bytes if the directory exists.
        payload_bytes: Option<u64>,
        /// Handshake wait in seconds when the plan would run.
        handshake_timeout_secs: Option<u64>,
        /// Remote directory that would receive staged assets.
        remote_dir: Option<String>,
        /// SSH connection summary for the plan.
        ssh: Option<BootstrapPlanSsh>,
        /// Environment variable keys that would be exported.
        env_keys: Vec<String>,
        /// Optional verification configuration summary.
        verify: Option<BootstrapPlanVerify>,
        /// Artifact hash spanning script, payload, env, and verify inputs.
        artifact_hash: Option<String>,
        /// Path to bootstrap metadata when discovered.
        metadata_path: Option<PathBuf>,
        /// Non-fatal warnings associated with the plan.
        warnings: Vec<String>,
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
        /// Legacy stamp identifier (reserved for compatibility; always `None`).
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

/// Planned managed image artifact verification entry.
#[derive(Debug, Clone)]
pub struct ManagedImageArtifactPlan {
    /// Artifact kind (root disk, kernel, initrd, ...).
    pub kind: ManagedArtifactKind,
    /// Artifact filename as referenced in the manifest.
    pub filename: String,
    /// Resolved filesystem path, when known.
    pub path: Option<PathBuf>,
    /// Expected SHA-256 checksum for the artifact.
    pub expected_sha256: Option<String>,
    /// Expected size in bytes for the artifact.
    pub expected_size_bytes: Option<u64>,
}

/// Verification summary for a managed image artifact.
#[derive(Debug, Clone)]
pub struct ManagedImageArtifactReport {
    /// Artifact kind (root disk, kernel, initrd, ...).
    pub kind: ManagedArtifactKind,
    /// Artifact filename as recorded in the manifest.
    pub filename: String,
    /// Resolved filesystem path, when known.
    pub path: Option<PathBuf>,
    /// Observed size in bytes for the artifact.
    pub size_bytes: Option<u64>,
    /// Checksums recorded during verification.
    pub checksums: Vec<ManagedImageChecksum>,
}

/// Recorded checksum for a managed image artifact.
#[derive(Debug, Clone)]
pub struct ManagedImageChecksum {
    /// Algorithm label (e.g. `sha256`, `source_sha256`).
    pub algo: String,
    /// Hex-encoded checksum value.
    pub value: String,
}

/// Evidence linking cleanup actions to prior managed image verification events.
#[derive(Debug, Clone)]
pub struct CleanupManagedImageEvidence {
    /// Managed image identifier.
    pub image_id: String,
    /// Managed image version associated with the verification entry.
    pub image_version: String,
    /// Filesystem path to the managed image root disk.
    pub root_disk_path: PathBuf,
    /// Path to the log containing the verification record.
    pub log_path: PathBuf,
    /// Timestamp (UTC seconds since epoch) when verification completed.
    pub verified_at: SystemTime,
    /// Total artifact bytes recorded in the verification result (if present).
    pub total_bytes: Option<u64>,
    /// Artifact filenames recorded in the verification result.
    pub artifacts: Vec<String>,
    /// Absolute difference between verification completion and current on-disk timestamp.
    pub verification_delta: Option<Duration>,
}

/// Trigger that initiated a bootstrap run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootstrapTrigger {
    /// Run initiated in automatic mode after detecting changes.
    Auto,
    /// Run explicitly requested regardless of automatic heuristics.
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
    /// Verifying the bootstrap outcome using remote checks or runner signals.
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
    /// Bootstrap runner reported no additional work was required.
    NoOp,
}

/// Dry-run action that a bootstrap plan would take.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootstrapPlanAction {
    /// Pipeline would attempt to run (auto or always).
    WouldRun,
    /// Pipeline would be skipped (skip mode or auto without a script).
    WouldSkip,
    /// Pipeline would fail due to configuration errors.
    Error,
}

impl BootstrapPlanAction {
    /// Human-friendly description.
    pub fn describe(self) -> &'static str {
        match self {
            BootstrapPlanAction::WouldRun => "would run",
            BootstrapPlanAction::WouldSkip => "would skip",
            BootstrapPlanAction::Error => "would error",
        }
    }

    /// Whether the plan represents an error state.
    pub fn is_error(self) -> bool {
        matches!(self, BootstrapPlanAction::Error)
    }
}

/// SSH configuration surfaced as part of a bootstrap plan.
#[derive(Debug, Clone)]
pub struct BootstrapPlanSsh {
    pub user: String,
    pub host: String,
    pub port: u16,
    pub identity: Option<PathBuf>,
    pub options: Vec<String>,
}

impl BootstrapPlanSsh {
    /// Format as `user@host:port`.
    pub fn summary(&self) -> String {
        format!("{}@{}:{}", self.user, self.host, self.port)
    }
}

/// Verification configuration surfaced in a bootstrap plan.
#[derive(Debug, Clone)]
pub struct BootstrapPlanVerify {
    pub command: Option<String>,
    pub path: Option<String>,
    pub path_is_relative: bool,
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

/// Context explaining why an ephemeral overlay was discarded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EphemeralCleanupReason {
    /// Cleanup performed as part of an orderly shutdown.
    Shutdown,
    /// Cleanup performed when reclaiming leftovers from a prior crash or aborted run.
    Orphan,
}

impl EphemeralCleanupReason {
    /// Human-friendly label for rendering.
    pub fn describe(self) -> &'static str {
        match self {
            EphemeralCleanupReason::Shutdown => "shutdown",
            EphemeralCleanupReason::Orphan => "orphan-recovery",
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
