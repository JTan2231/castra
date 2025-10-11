mod cli;
mod error;

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{CommandFactory, Parser, error::ErrorKind};

use crate::cli::{Cli, Commands, DownArgs, InitArgs, LogsArgs, PortsArgs, StatusArgs, UpArgs};
use crate::error::{CliError, CliResult};

fn main() -> ExitCode {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(err) => {
            let _ = err.print();
            return match err.kind() {
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion => ExitCode::SUCCESS,
                _ => ExitCode::from(64),
            };
        }
    };

    let Cli { config, command } = cli;

    let command = match command {
        Some(cmd) => cmd,
        None => {
            let mut command = Cli::command();
            let _ = command.print_help();
            println!();
            return ExitCode::from(64);
        }
    };

    let exit = match command {
        Commands::Init(args) => handle_init(args, config.as_ref()),
        Commands::Up(args) => handle_up(args, config.as_ref()),
        Commands::Down(args) => handle_down(args, config.as_ref()),
        Commands::Status(args) => handle_status(args, config.as_ref()),
        Commands::Ports(args) => handle_ports(args, config.as_ref()),
        Commands::Logs(args) => handle_logs(args, config.as_ref()),
    };

    match exit {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("Error: {err}");
            err.exit_code()
        }
    }
}

fn handle_init(args: InitArgs, config_override: Option<&PathBuf>) -> CliResult<()> {
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
        .map(Path::to_path_buf)
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
    println!("  • Update `base_image` in the config to point at your QCOW2 base image.");
    println!("  • Run `castra up` once the image is prepared.");

    Ok(())
}

fn handle_up(args: UpArgs, config_override: Option<&PathBuf>) -> CliResult<()> {
    let _config = resolve_config_path(config_override, args.skip_discovery)?;
    not_yet(
        "VM lifecycle management",
        ".vizier/todo_qemu_lifecycle_minimal.md",
    )
}

fn handle_down(args: DownArgs, config_override: Option<&PathBuf>) -> CliResult<()> {
    let _config = resolve_config_path(config_override, args.skip_discovery)?;
    not_yet(
        "Graceful VM shutdown",
        ".vizier/todo_qemu_lifecycle_minimal.md",
    )
}

fn handle_status(args: StatusArgs, config_override: Option<&PathBuf>) -> CliResult<()> {
    let _config = resolve_config_path(config_override, args.skip_discovery)?;
    not_yet(
        "Status reporting",
        ".vizier/todo_observability_and_status_copy.md",
    )
}

fn handle_ports(args: PortsArgs, config_override: Option<&PathBuf>) -> CliResult<()> {
    let _config = resolve_config_path(config_override, false)?;
    let doc = if args.verbose {
        ".vizier/todo_networking_ergonomics_v1.md"
    } else {
        ".vizier/todo_networking_ergonomics_v1.md"
    };
    not_yet("Port listing", doc)
}

fn handle_logs(args: LogsArgs, config_override: Option<&PathBuf>) -> CliResult<()> {
    let _config = resolve_config_path(config_override, false)?;
    let _ = args;
    not_yet(
        "Integrated log streaming",
        ".vizier/todo_observability_and_status_copy.md",
    )
}

fn preferred_config_target(
    config_override: Option<&PathBuf>,
    output_flag: Option<&PathBuf>,
) -> PathBuf {
    if let Some(path) = output_flag {
        return path.clone();
    }
    if let Some(path) = config_override {
        return path.clone();
    }
    PathBuf::from("castra.toml")
}

fn default_project_name(target_path: &Path) -> String {
    if let Some(parent) = target_path.parent().filter(|p| !p.as_os_str().is_empty()) {
        if let Some(name) = parent.file_name().and_then(OsStr::to_str) {
            if !name.is_empty() {
                return name.to_string();
            }
        }
    }

    std::env::current_dir()
        .ok()
        .and_then(|path| {
            path.file_name()
                .and_then(OsStr::to_str)
                .map(ToString::to_string)
        })
        .unwrap_or_else(|| "castra-project".to_string())
}

fn default_config_contents(project_name: &str) -> String {
    format!(
        r#"# Castra project configuration
# Visit .vizier/todo_project_config_and_discovery.md for the roadmap.
version = "0.1.0"

[project]
name = "{project_name}"

[[vms]]
name = "devbox"
description = "Primary development VM"
base_image = "images/devbox-base.qcow2"
overlay = ".castra/devbox-overlay.qcow2"
cpus = 2
memory = "2048 MiB"

  [[vms.port_forwards]]
  host = 2222
  guest = 22
  protocol = "tcp"

  [[vms.port_forwards]]
  host = 8080
  guest = 80
  protocol = "tcp"

[workflows]
init = ["qemu-img create -f qcow2 -b {{{{ base_image }}}} {{{{ overlay }}}}"]
"#
    )
}

fn not_yet(feature: &'static str, tracking: &'static str) -> CliResult<()> {
    Err(CliError::NotYetImplemented { feature, tracking })
}

fn resolve_config_path(
    config_override: Option<&PathBuf>,
    skip_discovery: bool,
) -> CliResult<PathBuf> {
    if let Some(path) = config_override {
        if path.is_file() {
            return Ok(path.clone());
        } else {
            return Err(CliError::ExplicitConfigMissing { path: path.clone() });
        }
    }

    if skip_discovery {
        let cwd = current_dir()?;
        return Err(CliError::ConfigDiscoveryFailed { search_root: cwd });
    }

    let cwd = current_dir()?;
    discover_config(&cwd).ok_or_else(|| CliError::ConfigDiscoveryFailed { search_root: cwd })
}

fn current_dir() -> CliResult<PathBuf> {
    std::env::current_dir().map_err(|source| CliError::WorkingDirectoryUnavailable { source })
}

fn discover_config(start: &Path) -> Option<PathBuf> {
    let mut cursor = Some(start.to_path_buf());
    while let Some(dir) = cursor {
        let candidate = dir.join("castra.toml");
        if candidate.is_file() {
            return Some(candidate);
        }
        cursor = dir.parent().map(Path::to_path_buf);
    }
    None
}
