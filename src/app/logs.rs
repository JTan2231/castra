use std::io::{self, IsTerminal};
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

use crate::Result;
use crate::cli::LogsArgs;
use crate::core::operations;
use crate::core::options::LogsOptions;
use crate::core::outcome::{LogEntry, LogFollower, LogSection, LogSectionState};
use crate::core::project::format_config_warnings;

use super::common::{config_load_options, emit_diagnostics, split_config_warnings};

pub fn handle_logs(args: LogsArgs, config_override: Option<&PathBuf>) -> Result<()> {
    let options = LogsOptions {
        config: config_load_options(config_override, false),
        tail: args.tail,
        follow: args.follow,
    };

    let output = operations::logs(options.clone(), None)?;

    let (config_warnings, other) = split_config_warnings(&output.diagnostics);
    if let Some(message) = format_config_warnings(&config_warnings) {
        eprint!("{message}");
    }
    emit_diagnostics(&other);

    let use_color = io::stdout().is_terminal();
    println!(
        "Tailing last {} lines per source.{}",
        options.tail,
        if options.follow {
            " Press Ctrl-C to stop."
        } else {
            ""
        }
    );
    println!();

    for (idx, section) in output.value.sections.iter().enumerate() {
        render_section(section, use_color);
        if idx + 1 < output.value.sections.len() {
            println!();
        }
    }

    if options.follow {
        if let Some(mut follower) = output.value.follower {
            follow_logs(&mut follower, use_color)?;
        }
    }

    Ok(())
}

fn render_section(section: &LogSection, use_color: bool) {
    let prefix = format_log_prefix(&section.label, use_color);
    match section.state {
        LogSectionState::NotCreated => {
            println!("{prefix} (log file not created yet)");
        }
        LogSectionState::Empty => {
            println!("{prefix} (no log entries yet)");
        }
        LogSectionState::HasEntries => {
            print_entries(&prefix, &section.entries);
        }
    }
}

fn print_entries(prefix: &str, entries: &[LogEntry]) {
    if entries.is_empty() {
        println!("{prefix} (no log entries yet)");
        return;
    }

    for entry in entries {
        match &entry.line {
            Some(line) if line.is_empty() => println!("{prefix}"),
            Some(line) => println!("{prefix} {line}"),
            None => println!("{prefix}"),
        }
    }
}

fn follow_logs(follower: &mut LogFollower, use_color: bool) -> Result<()> {
    println!("--- Following logs (press Ctrl-C to stop) ---");
    loop {
        let updates = follower.poll()?;
        if updates.is_empty() {
            thread::sleep(Duration::from_millis(250));
            continue;
        }
        for (label, line) in updates {
            let prefix = format_log_prefix(&label, use_color);
            match line {
                Some(text) if text.is_empty() => println!("{prefix}"),
                Some(text) => println!("{prefix} {text}"),
                None => println!("{prefix}"),
            }
        }
    }
}

fn format_log_prefix(label: &str, colored: bool) -> String {
    let bracketed = format!("[{label}]");
    if !colored {
        return bracketed;
    }

    let code = if label.starts_with("host-broker") {
        "36"
    } else if label.contains(":serial") {
        "35"
    } else {
        "34"
    };
    colorize(&bracketed, code)
}

fn colorize(text: &str, code: &str) -> String {
    format!("\u{001b}[{code}m{text}\u{001b}[0m")
}
