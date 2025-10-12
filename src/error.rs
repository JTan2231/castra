use std::path::PathBuf;

use thiserror::Error;

/// Convenient result alias using the library's error type.
pub type Result<T> = std::result::Result<T, Error>;

/// Castra library error type.
#[derive(Debug, Error)]
pub enum Error {
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
    #[error("Configuration validation failed for {path}: {message}")]
    InvalidConfig { path: PathBuf, message: String },
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
    #[error("Preflight failed: {message}")]
    PreflightFailed { message: String },
    #[error("Failed to launch VM `{vm}`: {message}")]
    LaunchFailed { vm: String, message: String },
    #[error("Failed to shut down VM `{vm}`: {message}")]
    ShutdownFailed { vm: String, message: String },
    #[error("Failed to read logs at {path}: {source}")]
    LogReadFailed {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}
