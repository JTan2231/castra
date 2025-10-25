use std::path::PathBuf;

use crate::core::diagnostics::{Diagnostic, Severity};
use crate::core::options::{ConfigLoadOptions, ConfigSource};
use crate::{Error, Result};

pub fn config_source(config_override: Option<&PathBuf>) -> ConfigSource {
    match config_override {
        Some(path) => ConfigSource::Explicit(path.clone()),
        None => ConfigSource::Discover,
    }
}

pub fn config_load_options(
    config_override: Option<&PathBuf>,
    skip_discovery: bool,
    command: &'static str,
) -> Result<ConfigLoadOptions> {
    if skip_discovery && config_override.is_none() {
        return Err(Error::SkipDiscoveryRequiresConfig { command });
    }

    match config_override.map(|p| ConfigSource::Explicit(p.clone())) {
        Some(source) => Ok(ConfigLoadOptions {
            source,
            allow_synthetic: !skip_discovery,
            search_root: None,
        }),
        None => Ok(ConfigLoadOptions::discover(!skip_discovery)),
    }
}

pub fn split_config_warnings(diagnostics: &[Diagnostic]) -> (Vec<Diagnostic>, Vec<Diagnostic>) {
    let mut config = Vec::new();
    let mut rest = Vec::new();
    for diagnostic in diagnostics {
        if matches!(diagnostic.severity, Severity::Warning) && diagnostic.path.is_some() {
            config.push(diagnostic.clone());
        } else {
            rest.push(diagnostic.clone());
        }
    }
    (config, rest)
}

pub fn emit_diagnostics(diagnostics: &[Diagnostic]) {
    for diagnostic in diagnostics {
        match diagnostic.severity {
            Severity::Warning => {
                eprintln!("Warning: {}", diagnostic.message);
                if let Some(help) = &diagnostic.help {
                    eprintln!("         {help}");
                }
            }
            Severity::Info => {
                println!("{}", diagnostic.message);
                if let Some(help) = &diagnostic.help {
                    println!("{help}");
                }
            }
            Severity::Error => {
                eprintln!("Error: {}", diagnostic.message);
                if let Some(help) = &diagnostic.help {
                    eprintln!("       {help}");
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::options::ConfigSource;
    use std::path::PathBuf;

    #[test]
    fn skip_discovery_requires_explicit_config() {
        let err = config_load_options(None, true, "status").unwrap_err();
        match err {
            Error::SkipDiscoveryRequiresConfig { command } => {
                assert_eq!(command, "status");
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn clean_skip_discovery_requires_explicit_config() {
        let err = config_load_options(None, true, "clean").unwrap_err();
        match err {
            Error::SkipDiscoveryRequiresConfig { command } => {
                assert_eq!(command, "clean");
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn explicit_config_passthrough() {
        let path = PathBuf::from("castra.toml");
        let opts = config_load_options(Some(&path), true, "up").expect("config options");
        match &opts.source {
            ConfigSource::Explicit(explicit) => assert_eq!(explicit, &path),
            _ => panic!("expected explicit source"),
        }
        assert!(!opts.allow_synthetic);
    }

    #[test]
    fn discovery_allowed_when_skip_disabled() {
        let opts = config_load_options(None, false, "ports").expect("config options");
        assert!(matches!(opts.source, ConfigSource::Discover));
        assert!(opts.allow_synthetic);
    }
}
