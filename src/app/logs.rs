use std::collections::VecDeque;
use std::fs;
use std::io::{self, BufRead, BufReader, IsTerminal, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use crate::cli::LogsArgs;
use crate::error::{CliError, CliResult};

use super::display::colorize;
use super::project::{config_state_root, emit_config_warnings, load_or_default_project};

pub fn handle_logs(args: LogsArgs, config_override: Option<&PathBuf>) -> CliResult<()> {
    let project = load_or_default_project(config_override, false)?;

    emit_config_warnings(&project.warnings);

    let log_dir = config_state_root(&project).join("logs");
    let use_color = io::stdout().is_terminal();
    println!(
        "Tailing last {} lines per source.{}",
        args.tail,
        if args.follow {
            " Press Ctrl-C to stop."
        } else {
            ""
        }
    );
    println!();

    let mut sources = Vec::new();
    let mut sections: Vec<(String, PathBuf)> = Vec::new();
    sections.push(("host-broker".to_string(), log_dir.join("broker.log")));

    for vm in &project.vms {
        sections.push((
            format!("vm:{}:qemu", vm.name),
            log_dir.join(format!("{}.log", vm.name)),
        ));
        sections.push((
            format!("vm:{}:serial", vm.name),
            log_dir.join(format!("{}-serial.log", vm.name)),
        ));
    }

    for (idx, (label, path)) in sections.iter().enumerate() {
        let styled_prefix = format_log_prefix(label, use_color);
        let offset = emit_log_tail(&styled_prefix, path, args.tail)?;
        sources.push(LogSource {
            prefix: styled_prefix,
            path: path.clone(),
            offset,
        });
        if idx + 1 < sections.len() {
            println!();
        }
    }

    if args.follow {
        follow_logs(&mut sources)?;
    }

    Ok(())
}

struct LogSource {
    prefix: String,
    path: PathBuf,
    offset: u64,
}

fn follow_logs(sources: &mut [LogSource]) -> CliResult<()> {
    println!("--- Following logs (press Ctrl-C to stop) ---");
    loop {
        let mut activity = false;
        for source in sources.iter_mut() {
            match fs::File::open(&source.path) {
                Ok(mut file) => {
                    if source.offset > 0 {
                        if let Err(err) = file.seek(SeekFrom::Start(source.offset)) {
                            return Err(CliError::LogReadFailed {
                                path: source.path.clone(),
                                source: err,
                            });
                        }
                    }

                    let mut reader = BufReader::new(file);
                    let mut buffer = String::new();
                    loop {
                        buffer.clear();
                        let bytes = reader.read_line(&mut buffer).map_err(|err| {
                            CliError::LogReadFailed {
                                path: source.path.clone(),
                                source: err,
                            }
                        })?;
                        if bytes == 0 {
                            break;
                        }
                        source.offset += bytes as u64;
                        while buffer.ends_with('\n') || buffer.ends_with('\r') {
                            buffer.pop();
                        }
                        if buffer.is_empty() {
                            println!("{}", source.prefix);
                        } else {
                            println!("{} {buffer}", source.prefix);
                        }
                        activity = true;
                    }
                }
                Err(err) if err.kind() == io::ErrorKind::NotFound => {}
                Err(err) => {
                    return Err(CliError::LogReadFailed {
                        path: source.path.clone(),
                        source: err,
                    });
                }
            }
        }

        if !activity {
            thread::sleep(Duration::from_millis(250));
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
    colorize(&bracketed, code, colored)
}

fn emit_log_tail(prefix: &str, path: &Path, tail: usize) -> CliResult<u64> {
    if tail > 0 {
        match read_tail_lines(path, tail) {
            Ok(lines) if lines.is_empty() => {
                if path.exists() {
                    println!("{prefix} (no log entries yet)");
                } else {
                    println!("{prefix} (log file not created yet)");
                }
            }
            Ok(lines) => {
                for line in lines {
                    if line.is_empty() {
                        println!("{prefix}");
                    } else {
                        println!("{prefix} {line}");
                    }
                }
            }
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                println!("{prefix} (log file not created yet)");
            }
            Err(err) => {
                return Err(CliError::LogReadFailed {
                    path: path.to_path_buf(),
                    source: err,
                });
            }
        }
    } else if !path.exists() {
        println!("{prefix} (log file not created yet)");
    }

    let offset = fs::metadata(path).map(|meta| meta.len()).unwrap_or(0);
    Ok(offset)
}

fn read_tail_lines(path: &Path, limit: usize) -> io::Result<Vec<String>> {
    if limit == 0 {
        return Ok(Vec::new());
    }

    let file = fs::File::open(path)?;
    let reader = BufReader::new(file);
    let mut ring: VecDeque<String> = VecDeque::with_capacity(limit);

    for line in reader.lines() {
        let line = line?;
        if ring.len() == limit {
            ring.pop_front();
        }
        ring.push_back(line);
    }

    Ok(ring.into_iter().collect())
}
