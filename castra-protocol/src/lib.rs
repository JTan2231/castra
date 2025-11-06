use std::sync::OnceLock;

use semver::Version;

/// Minimum supported vizier remote protocol version (inclusive).
pub const VIZIER_REMOTE_PROTOCOL_MIN: &str = "1.0.0";
/// Maximum supported vizier remote protocol version (exclusive).
pub const VIZIER_REMOTE_PROTOCOL_MAX: &str = "2.0.0";
/// Human-readable description of the supported protocol range.
pub const VIZIER_REMOTE_PROTOCOL_RANGE: &str = ">=1.0.0, <2.0.0";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtocolCompatibility {
    Supported,
    BelowMinimum,
    AboveMaximum,
}

/// Evaluate whether a vizier remote protocol version is supported.
pub fn check_protocol_version(version: &str) -> Result<ProtocolCompatibility, semver::Error> {
    let parsed = Version::parse(version)?;
    let min = min_version();
    let max = max_version();
    if parsed < *min {
        Ok(ProtocolCompatibility::BelowMinimum)
    } else if parsed >= *max {
        Ok(ProtocolCompatibility::AboveMaximum)
    } else {
        Ok(ProtocolCompatibility::Supported)
    }
}

/// Return the supported protocol range as a human-readable string.
pub fn supported_protocol_range() -> &'static str {
    VIZIER_REMOTE_PROTOCOL_RANGE
}

fn min_version() -> &'static Version {
    static MIN: OnceLock<Version> = OnceLock::new();
    MIN.get_or_init(|| {
        Version::parse(VIZIER_REMOTE_PROTOCOL_MIN).expect("valid VIZIER_REMOTE_PROTOCOL_MIN semver")
    })
}

fn max_version() -> &'static Version {
    static MAX: OnceLock<Version> = OnceLock::new();
    MAX.get_or_init(|| {
        Version::parse(VIZIER_REMOTE_PROTOCOL_MAX).expect("valid VIZIER_REMOTE_PROTOCOL_MAX semver")
    })
}
