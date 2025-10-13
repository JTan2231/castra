use std::path::PathBuf;

use crate::Result;
use crate::cli::UpArgs;
use crate::core::diagnostics::Severity;
use crate::core::events::Event;
use crate::core::operations;
use crate::core::options::UpOptions;
use crate::core::outcome::UpOutcome;
use crate::core::project::format_config_warnings;

use super::common::{config_load_options, emit_diagnostics, split_config_warnings};

pub fn handle_up(args: UpArgs, config_override: Option<&PathBuf>) -> Result<()> {
    let options = UpOptions {
        config: config_load_options(config_override, args.skip_discovery, "up")?,
        force: args.force,
    };

    let output = operations::up(options, None)?;

    let (config_warnings, other) = split_config_warnings(&output.diagnostics);
    if let Some(message) = format_config_warnings(&config_warnings) {
        eprint!("{message}");
    }
    emit_diagnostics(&other);

    render_up(&output.value, &output.events);

    Ok(())
}

fn render_up(outcome: &UpOutcome, events: &[Event]) {
    for event in events {
        match event {
            Event::ManagedArtifact { spec, text, .. } => {
                println!("→ {} {}: {}", spec.id, spec.version, text);
            }
            Event::ManagedImageVerified { spec, artifacts } => {
                let kinds: Vec<&str> = artifacts
                    .iter()
                    .map(|artifact| artifact.kind.describe())
                    .collect();
                if kinds.is_empty() {
                    println!(
                        "→ {} {}: verified managed artifacts.",
                        spec.id, spec.version
                    );
                } else {
                    println!(
                        "→ {} {}: verified managed artifacts ({}).",
                        spec.id,
                        spec.version,
                        kinds.join(", ")
                    );
                }
            }
            Event::ManagedImageProfileApplied {
                spec,
                vm,
                initrd,
                machine,
                ..
            } => {
                let mut components = vec!["kernel".to_string()];
                if initrd.is_some() {
                    components.push("initrd".to_string());
                }
                if let Some(machine) = machine {
                    components.push(format!("machine={machine}"));
                }
                println!(
                    "→ {} {}: applied boot profile for VM `{}` ({}).",
                    spec.id,
                    spec.version,
                    vm,
                    components.join(", ")
                );
            }
            Event::OverlayPrepared { vm, overlay_path } => {
                println!(
                    "Prepared overlay for VM `{vm}` at {}.",
                    overlay_path.display()
                );
            }
            Event::VmLaunched { vm, .. } => {
                let pidfile = outcome.state_root.join(format!("{vm}.pid"));
                println!("→ {vm}: launched (pidfile {}).", pidfile.display());
            }
            Event::BrokerStarted { pid, port } => {
                println!("→ broker: launched on 127.0.0.1:{port} (pid {pid}).");
            }
            Event::Message { severity, text } => match severity {
                Severity::Info => println!("{}", text),
                Severity::Warning => eprintln!("Warning: {}", text),
                Severity::Error => eprintln!("Error: {}", text),
            },
            _ => {}
        }
    }
}
