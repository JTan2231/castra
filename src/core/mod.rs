//! Core Castra library API surface.

pub mod diagnostics;
pub mod events;
pub mod options;
pub mod outcome;
pub mod reporter;

pub mod broker;
pub mod logs;
pub mod operations;
pub mod ports;
pub mod project;
pub mod runtime;
pub mod status;

pub use diagnostics::{Diagnostic, Severity};
pub use events::{CleanupKind, Event, ManagedImageSpecHandle};
pub use operations::{broker, clean, down, init, logs, ports, status, up};
pub use options::{
    BrokerOptions, CleanOptions, CleanScope, ConfigLoadOptions, ConfigSource, DownOptions,
    InitOptions, LogsOptions, PortsOptions, PortsView, ProjectSelector, StatusOptions, UpOptions,
};
pub use outcome::{
    BrokerLaunchOutcome, BrokerShutdownOutcome, BrokerState, CleanOutcome, CleanupAction,
    DownOutcome, InitOutcome, LogEntry, LogFollower, LogSection, LogSectionState, LogsOutcome,
    OperationOutput, OperationResult, PortConflictRow, PortForwardRow, PortForwardStatus,
    PortsOutcome, SkipReason, StateRootCleanup, StatusOutcome, UpOutcome, VmLaunchOutcome,
    VmPortDetail, VmShutdownOutcome,
};
pub use reporter::Reporter;
