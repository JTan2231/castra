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
pub use events::{Event, ManagedImageSpecHandle};
pub use operations::{broker, down, init, logs, ports, status, up};
pub use options::{
    BrokerOptions, ConfigLoadOptions, ConfigSource, DownOptions, InitOptions, LogsOptions,
    PortsOptions, PortsView, StatusOptions, UpOptions,
};
pub use outcome::{
    BrokerLaunchOutcome, BrokerShutdownOutcome, BrokerState, DownOutcome, InitOutcome, LogEntry,
    LogFollower, LogSection, LogSectionState, LogsOutcome, OperationOutput, OperationResult,
    PortConflictRow, PortForwardRow, PortForwardStatus, PortsOutcome, StatusOutcome, UpOutcome,
    VmLaunchOutcome, VmPortDetail, VmShutdownOutcome,
};
pub use reporter::Reporter;
