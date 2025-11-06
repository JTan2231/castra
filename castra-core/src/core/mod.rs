//! Core Castra library API surface.

pub mod diagnostics;
pub mod events;
pub mod options;
pub mod outcome;
pub mod reporter;

pub mod bootstrap;
pub mod logs;
pub mod operations;
pub mod ports;
pub mod project;
pub mod runtime;
pub mod status;
pub mod workspace_registry;

pub use diagnostics::{Diagnostic, Severity};
pub use events::{CleanupKind, Event};
pub use operations::{clean, down, init, logs, ports, status, up, up_with_launcher};
pub use options::{
    CleanOptions, CleanScope, ConfigLoadOptions, ConfigSource, DownOptions, InitOptions,
    LogsOptions, PortsOptions, PortsView, ProjectSelector, StatusOptions, UpOptions, VmLaunchMode,
};
pub use outcome::{
    BootstrapRunOutcome, BootstrapRunStatus, CleanOutcome, CleanupAction, DownOutcome, InitOutcome,
    LogEntry, LogFollower, LogSection, LogSectionState, LogsOutcome, OperationOutput,
    OperationResult, PortConflictRow, PortForwardRow, PortForwardStatus, PortInactiveReason,
    PortsOutcome, ProjectPortsOutcome, SkipReason, StateRootCleanup, StatusOutcome, UpOutcome,
    VmLaunchOutcome, VmPortDetail, VmShutdownOutcome,
};
pub use reporter::Reporter;
pub use runtime::{ProcessVizierLauncher, VizierLaunchRequest, VizierLauncher};
