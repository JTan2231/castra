use std::path::PathBuf;

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
    },
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
    /// Cached base images or downloaded artifacts.
    Images,
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
            CleanupKind::Images => "images",
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
