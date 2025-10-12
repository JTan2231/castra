use std::path::PathBuf;

use crate::managed::{ManagedArtifactKind, ManagedImageSpec};

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
    /// Status update for managed artifact acquisition.
    ManagedArtifact {
        /// The artifact specification that is being provisioned.
        spec: ManagedImageSpecHandle,
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
    /// Notification that a VM changed shutdown status. `stopped` indicates whether the VM is now halted.
    VmShutdown {
        /// Name of the VM.
        vm: String,
        /// Whether a change occurred (`true` if the VM transitioned state, `false` if it was already stopped).
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
