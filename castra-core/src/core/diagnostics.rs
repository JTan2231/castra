use std::path::PathBuf;

/// Severity level of a diagnostic emitted by Castra operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// Informational message with no required action.
    Info,
    /// Warning that signals potential issues but allows the workflow to continue.
    Warning,
    /// Error-level diagnostic. Library operations normally return `Result::Err` for hard failures,
    /// but this variant is provided for completeness when additional context is useful.
    Error,
}

/// Structured diagnostic surfaced alongside operation outcomes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// Severity of the diagnostic message.
    pub severity: Severity,
    /// Human-readable description.
    pub message: String,
    /// Optional path that the diagnostic refers to (configuration file, log file, etc.).
    pub path: Option<PathBuf>,
    /// Optional hint to help callers remediate the issue.
    pub help: Option<String>,
}

impl Diagnostic {
    /// Produce a new diagnostic with the provided severity and message.
    pub fn new<S: Into<String>>(severity: Severity, message: S) -> Self {
        Self {
            severity,
            message: message.into(),
            path: None,
            help: None,
        }
    }

    /// Attach a filesystem path to the diagnostic.
    pub fn with_path(mut self, path: PathBuf) -> Self {
        self.path = Some(path);
        self
    }

    /// Attach a remediation hint to the diagnostic.
    pub fn with_help<S: Into<String>>(mut self, help: S) -> Self {
        self.help = Some(help.into());
        self
    }
}
