use std::path::PathBuf;

use crate::core::diagnostics::{Diagnostic, Severity};
use crate::core::options::{ConfigLoadOptions, ConfigSource};

pub fn config_source(config_override: Option<&PathBuf>) -> ConfigSource {
    match config_override {
        Some(path) => ConfigSource::Explicit(path.clone()),
        None => ConfigSource::Discover,
    }
}

pub fn config_load_options(
    config_override: Option<&PathBuf>,
    skip_discovery: bool,
) -> ConfigLoadOptions {
    match config_override.map(|p| ConfigSource::Explicit(p.clone())) {
        Some(source) => ConfigLoadOptions {
            source,
            allow_synthetic: !skip_discovery,
            search_root: None,
        },
        None => ConfigLoadOptions::discover(!skip_discovery),
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
