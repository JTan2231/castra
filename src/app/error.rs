use std::process::ExitCode;

use crate::Error;

pub fn exit_code(err: &Error) -> ExitCode {
    match err {
        Error::AlreadyInitialized { .. } => ExitCode::from(73),
        Error::CreateDir { .. } => ExitCode::from(73),
        Error::WriteConfig { .. } => ExitCode::from(74),
        Error::ReadConfig { .. } => ExitCode::from(74),
        Error::ParseConfig { .. } => ExitCode::from(65),
        Error::InvalidConfig { .. } => ExitCode::from(65),
        Error::ExplicitConfigMissing { .. } => ExitCode::from(66),
        Error::ConfigDiscoveryFailed { .. } => ExitCode::from(66),
        Error::NoActiveWorkspaces => ExitCode::from(66),
        Error::WorkspaceNotFound { .. } => ExitCode::from(66),
        Error::WorkspaceConfigUnavailable { .. } => ExitCode::from(70),
        Error::WorkingDirectoryUnavailable { .. } => ExitCode::from(70),
        Error::SkipDiscoveryRequiresConfig { .. } => ExitCode::from(64),
        Error::PreflightFailed { .. } => ExitCode::from(70),
        Error::LaunchFailed { .. } => ExitCode::from(70),
        Error::ShutdownFailed { .. } => ExitCode::from(70),
        Error::BootstrapFailed { .. } => ExitCode::from(70),
        Error::BusPublishFailed { .. } => ExitCode::from(70),
        Error::LogReadFailed { .. } => ExitCode::from(74),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;

    #[test]
    fn exit_code_matches_expected_values() {
        assert_eq!(
            exit_code(&Error::AlreadyInitialized {
                path: "config".into()
            }),
            ExitCode::from(73)
        );
        assert_eq!(
            exit_code(&Error::CreateDir {
                path: "dir".into(),
                source: io::Error::new(io::ErrorKind::Other, "err")
            }),
            ExitCode::from(73)
        );
        assert_eq!(
            exit_code(&Error::WriteConfig {
                path: "file".into(),
                source: io::Error::new(io::ErrorKind::Other, "err")
            }),
            ExitCode::from(74)
        );
        assert_eq!(
            exit_code(&Error::ParseConfig {
                path: "file".into(),
                source: toml::from_str::<toml::Value>("invalid").unwrap_err()
            }),
            ExitCode::from(65)
        );
        assert_eq!(
            exit_code(&Error::ExplicitConfigMissing {
                path: "missing".into()
            }),
            ExitCode::from(66)
        );
        assert_eq!(
            exit_code(&Error::ConfigDiscoveryFailed {
                search_root: "root".into()
            }),
            ExitCode::from(66)
        );
        assert_eq!(
            exit_code(&Error::SkipDiscoveryRequiresConfig { command: "status" }),
            ExitCode::from(64)
        );
        assert_eq!(
            exit_code(&Error::WorkingDirectoryUnavailable {
                source: io::Error::new(io::ErrorKind::Other, "err")
            }),
            ExitCode::from(70)
        );
        assert_eq!(
            exit_code(&Error::PreflightFailed {
                message: "fail".into()
            }),
            ExitCode::from(70)
        );
        assert_eq!(
            exit_code(&Error::BootstrapFailed {
                vm: "vm".into(),
                message: "err".into()
            }),
            ExitCode::from(70)
        );
        assert_eq!(
            exit_code(&Error::BusPublishFailed {
                message: "oops".into()
            }),
            ExitCode::from(70)
        );
        assert_eq!(
            exit_code(&Error::LogReadFailed {
                path: "log".into(),
                source: io::Error::new(io::ErrorKind::Other, "err")
            }),
            ExitCode::from(74)
        );
    }
}
