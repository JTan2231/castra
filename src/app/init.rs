use std::fs;
use std::path::PathBuf;

use crate::cli::InitArgs;
use crate::error::{CliError, CliResult};

use super::project::{default_config_contents, default_project_name, preferred_config_target};

pub fn handle_init(args: InitArgs, config_override: Option<&PathBuf>) -> CliResult<()> {
    let target_path = preferred_config_target(config_override, args.output.as_ref());
    let project_name = args
        .project_name
        .clone()
        .unwrap_or_else(|| default_project_name(&target_path));

    if target_path.exists() && !args.force {
        return Err(CliError::AlreadyInitialized {
            path: target_path.clone(),
        });
    }

    if let Some(parent) = target_path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent).map_err(|source| CliError::CreateDir {
            path: parent.to_path_buf(),
            source,
        })?;
    }

    let workdir = target_path
        .parent()
        .map(std::path::Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".castra");

    fs::create_dir_all(&workdir).map_err(|source| CliError::CreateDir {
        path: workdir.clone(),
        source,
    })?;

    let config_contents = default_config_contents(&project_name);
    fs::write(&target_path, config_contents).map_err(|source| CliError::WriteConfig {
        path: target_path.clone(),
        source,
    })?;

    println!("✔ Created castra project scaffold.");
    println!("  config  → {}", target_path.display());
    println!("  workdir → {}", workdir.display());
    println!();
    println!("Next steps:");
    println!(
        "  • Update `base_image` or set `managed_image` in the config to choose your base disk."
    );
    println!("  • Run `castra up` once the image is prepared.");

    Ok(())
}
