use std::path::PathBuf;
use std::process::ExitCode;

use thiserror::Error;

pub type CliResult<T> = Result<T, CliError>;

#[derive(Debug, Error)]
pub enum CliError {
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

impl CliError {
    pub fn exit_code(&self) -> ExitCode {
        match self {
            Self::AlreadyInitialized { .. } => ExitCode::from(73),
            Self::CreateDir { .. } => ExitCode::from(73),
            Self::WriteConfig { .. } => ExitCode::from(74),
            Self::ReadConfig { .. } => ExitCode::from(74),
            Self::ParseConfig { .. } => ExitCode::from(65),
            Self::InvalidConfig { .. } => ExitCode::from(65),
            Self::ExplicitConfigMissing { .. } => ExitCode::from(66),
            Self::ConfigDiscoveryFailed { .. } => ExitCode::from(66),
            Self::WorkingDirectoryUnavailable { .. } => ExitCode::from(70),
            Self::PreflightFailed { .. } => ExitCode::from(70),
            Self::LaunchFailed { .. } => ExitCode::from(70),
            Self::ShutdownFailed { .. } => ExitCode::from(70),
            Self::LogReadFailed { .. } => ExitCode::from(74),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;

    #[test]
    fn exit_code_matches_expected_values() {
        assert_eq!(
            CliError::AlreadyInitialized {
                path: PathBuf::from("config")
            }
            .exit_code(),
            ExitCode::from(73)
        );
        assert_eq!(
            CliError::CreateDir {
                path: PathBuf::from("dir"),
                source: io::Error::new(io::ErrorKind::Other, "err"),
            }
            .exit_code(),
            ExitCode::from(73)
        );
        assert_eq!(
            CliError::WriteConfig {
                path: PathBuf::from("file"),
                source: io::Error::new(io::ErrorKind::Other, "err"),
            }
            .exit_code(),
            ExitCode::from(74)
        );
        assert_eq!(
            CliError::ParseConfig {
                path: PathBuf::from("file"),
                source: toml::from_str::<toml::Value>("invalid").unwrap_err(),
            }
            .exit_code(),
            ExitCode::from(65)
        );
        assert_eq!(
            CliError::ExplicitConfigMissing {
                path: PathBuf::from("missing")
            }
            .exit_code(),
            ExitCode::from(66)
        );
        assert_eq!(
            CliError::ConfigDiscoveryFailed {
                search_root: PathBuf::from("root")
            }
            .exit_code(),
            ExitCode::from(66)
        );
        assert_eq!(
            CliError::WorkingDirectoryUnavailable {
                source: io::Error::new(io::ErrorKind::Other, "err")
            }
            .exit_code(),
            ExitCode::from(70)
        );
        assert_eq!(
            CliError::PreflightFailed {
                message: "fail".into()
            }
            .exit_code(),
            ExitCode::from(70)
        );
        assert_eq!(
            CliError::LogReadFailed {
                path: PathBuf::from("log"),
                source: io::Error::new(io::ErrorKind::Other, "err")
            }
            .exit_code(),
            ExitCode::from(74)
        );
    }
}
