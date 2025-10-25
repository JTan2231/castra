use std::io::{self, IsTerminal};
use std::path::PathBuf;

use crate::Result;
use crate::cli::{BusArgs, BusCommands, BusPublishArgs, BusTailArgs};
use crate::core::operations;
use crate::core::options::{BusLogTarget, BusPublishOptions, BusTailOptions};
use crate::core::outcome::LogSectionState;
use crate::core::project::format_config_warnings;

use super::common::{config_load_options, emit_diagnostics, split_config_warnings};
use super::logs::{follow_logs, format_log_prefix, print_entries};

pub fn handle_bus(args: BusArgs, config_override: Option<&PathBuf>) -> Result<()> {
    match args.command {
        BusCommands::Publish(publish_args) => handle_bus_publish(publish_args, config_override),
        BusCommands::Tail(tail_args) => handle_bus_tail(tail_args, config_override),
    }
}

fn handle_bus_publish(args: BusPublishArgs, config_override: Option<&PathBuf>) -> Result<()> {
    let payload: serde_json::Value =
        serde_json::from_str(&args.payload).map_err(|err| crate::Error::PreflightFailed {
            message: format!("Failed to parse JSON payload: {err}"),
        })?;

    let options = BusPublishOptions {
        config: config_load_options(config_override, args.skip_discovery, "bus publish")?,
        topic: args.topic,
        payload,
    };

    let output = operations::bus_publish(options, None)?;
    let (config_warnings, other) = split_config_warnings(&output.diagnostics);
    if let Some(message) = format_config_warnings(&config_warnings) {
        eprint!("{message}");
    }
    emit_diagnostics(&other);

    println!(
        "Published bus frame to {} (topic {}).",
        output.value.log_path.display(),
        output.value.topic
    );
    Ok(())
}

fn handle_bus_tail(args: BusTailArgs, config_override: Option<&PathBuf>) -> Result<()> {
    let tail = args.tail;
    let follow = args.follow;

    let target = match (&args.vm, args.shared) {
        (Some(name), _) => BusLogTarget::Vm(name.clone()),
        (None, _) => BusLogTarget::Shared,
    };

    let options = BusTailOptions {
        config: config_load_options(config_override, args.skip_discovery, "bus tail")?,
        target,
        tail,
        follow,
    };

    let output = operations::bus_tail(options, None)?;
    let (config_warnings, other) = split_config_warnings(&output.diagnostics);
    if let Some(message) = format_config_warnings(&config_warnings) {
        eprint!("{message}");
    }
    emit_diagnostics(&other);

    let use_color = io::stdout().is_terminal();
    println!(
        "Project: {} ({}).",
        output.value.project_name,
        output.value.project_path.display()
    );
    println!(
        "Tailing last {} lines from {}.{}",
        tail,
        output.value.log_label,
        if follow { " Press Ctrl-C to stop." } else { "" }
    );
    println!();

    let prefix = format_log_prefix(&output.value.log_label, use_color);
    match output.value.state {
        LogSectionState::NotCreated => {
            println!("{prefix} (log file not created yet)");
        }
        LogSectionState::Empty => {
            println!("{prefix} (no log entries yet)");
        }
        LogSectionState::HasEntries => {
            print_entries(&prefix, &output.value.entries);
        }
    }

    if follow {
        if let Some(mut follower) = output.value.follower {
            follow_logs(&mut follower, use_color)?;
        }
    }

    Ok(())
}
