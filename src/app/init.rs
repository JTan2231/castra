use std::path::PathBuf;

use crate::Result;
use crate::cli::InitArgs;
use crate::core::operations;
use crate::core::options::InitOptions;
use crate::core::project::format_config_warnings;

use super::common::{config_source, emit_diagnostics, split_config_warnings};

pub fn handle_init(args: InitArgs, config_override: Option<&PathBuf>) -> Result<()> {
    let options = InitOptions {
        force: args.force,
        project_name: args.project_name.clone(),
        output_path: args.output.clone(),
        config_hint: config_source(config_override),
    };

    let output = operations::init(options, None)?;

    let (config_warnings, other) = split_config_warnings(&output.diagnostics);
    if let Some(message) = format_config_warnings(&config_warnings) {
        eprint!("{message}");
    }
    emit_diagnostics(&other);

    let outcome = output.value;
    let default_overlay_dir = outcome.state_root.join("overlays");
    println!("✔ Created castra project scaffold.");
    println!("  config  → {}", outcome.config_path.display());
    println!("  state   → {}", outcome.state_root.display());
    println!(
        "  local   → {} (opt-in via [project].state_dir)",
        outcome.overlay_root.display()
    );
    println!();
    println!("Next steps:");
    println!(
        "  • Edit [[vms]] if you need a base image other than the bundled `alpine-minimal@v1` or want overlays outside {}.",
        default_overlay_dir.display()
    );
    println!("  • Run `castra up` when you're ready to launch.");

    Ok(())
}
