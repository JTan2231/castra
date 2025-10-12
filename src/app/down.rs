use std::path::PathBuf;

use crate::Result;
use crate::cli::DownArgs;
use crate::core::diagnostics::Severity;
use crate::core::events::Event;
use crate::core::operations;
use crate::core::options::DownOptions;
use crate::core::project::format_config_warnings;

use super::common::{config_load_options, emit_diagnostics, split_config_warnings};

pub fn handle_down(args: DownArgs, config_override: Option<&PathBuf>) -> Result<()> {
    let options = DownOptions {
        config: config_load_options(config_override, args.skip_discovery),
    };

    let output = operations::down(options, None)?;

    let (config_warnings, other) = split_config_warnings(&output.diagnostics);
    if let Some(message) = format_config_warnings(&config_warnings) {
        eprint!("{message}");
    }
    emit_diagnostics(&other);

    render_down(&output.events);

    Ok(())
}

fn render_down(events: &[Event]) {
    for event in events {
        match event {
            Event::Message { severity, text } => match severity {
                Severity::Info => println!("{text}"),
                Severity::Warning => eprintln!("Warning: {text}"),
                Severity::Error => eprintln!("Error: {text}"),
            },
            Event::BrokerStopped { changed } => {
                if !changed {
                    println!("→ broker: already stopped.");
                } else {
                    println!("→ broker: stopped.");
                }
            }
            _ => {}
        }
    }
}
