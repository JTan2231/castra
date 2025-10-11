use std::path::PathBuf;
use std::process::ExitCode;

use thiserror::Error;

pub type CliResult<T> = Result<T, CliError>;

#[derive(Debug, Error)]
pub enum CliError {
    #[error("{feature} is not available yet. Track progress in {tracking}.")]
    NotYetImplemented {
        feature: &'static str,
        tracking: &'static str,
    },
    #[error(
        "A castra configuration already exists at {path}. \
         Re-run with --force to overwrite the generated files."
    )]
    AlreadyInitialized { path: PathBuf },
    #[error("Failed to create directory {path}: {source}")]
    CreateDir {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("Failed to write configuration file at {path}: {source}")]
    WriteConfig {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("Failed to read configuration file at {path}: {source}")]
    ReadConfig {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("Configuration at {path} could not be parsed: {source}")]
    ParseConfig {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },
    #[error("Configuration validation failed:\n{message}")]
    InvalidConfig { message: String },
    #[error("The configuration path {path} does not exist or is not readable.")]
    ExplicitConfigMissing { path: PathBuf },
    #[error(
        "No castra configuration found while searching upward from {search_root}. \
         Run `castra init` first or provide a path with --config."
    )]
    ConfigDiscoveryFailed { search_root: PathBuf },
    #[error("Failed to determine the current working directory: {source}")]
    WorkingDirectoryUnavailable {
        #[source]
        source: std::io::Error,
    },
}

impl CliError {
    pub fn exit_code(&self) -> ExitCode {
        match self {
            Self::NotYetImplemented { .. } => ExitCode::from(70),
            Self::AlreadyInitialized { .. } => ExitCode::from(73),
            Self::CreateDir { .. } => ExitCode::from(73),
            Self::WriteConfig { .. } => ExitCode::from(74),
            Self::ReadConfig { .. } => ExitCode::from(74),
            Self::ParseConfig { .. } => ExitCode::from(65),
            Self::InvalidConfig { .. } => ExitCode::from(65),
            Self::ExplicitConfigMissing { .. } => ExitCode::from(66),
            Self::ConfigDiscoveryFailed { .. } => ExitCode::from(66),
            Self::WorkingDirectoryUnavailable { .. } => ExitCode::from(70),
        }
    }
}
