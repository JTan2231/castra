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
        let activity = poll_log_sources(sources)?;
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

fn poll_log_sources(sources: &mut [LogSource]) -> CliResult<bool> {
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
                    let bytes =
                        reader
                            .read_line(&mut buffer)
                            .map_err(|err| CliError::LogReadFailed {
                                path: source.path.clone(),
                                source: err,
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
    Ok(activity)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::{NamedTempFile, TempDir};

    #[test]
    fn read_tail_lines_returns_last_entries() {
        let file = NamedTempFile::new().unwrap();
        writeln!(file.as_file(), "one").unwrap();
        writeln!(file.as_file(), "two").unwrap();
        writeln!(file.as_file(), "three").unwrap();
        let lines = read_tail_lines(file.path(), 2).unwrap();
        assert_eq!(lines, vec!["two".to_string(), "three".to_string()]);
    }

    #[test]
    fn read_tail_lines_zero_limit_is_empty() {
        let file = NamedTempFile::new().unwrap();
        let lines = read_tail_lines(file.path(), 0).unwrap();
        assert!(lines.is_empty());
    }

    #[test]
    fn emit_log_tail_reports_missing_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("missing.log");
        let offset = emit_log_tail("[label]", &path, 10).unwrap();
        assert_eq!(offset, 0);
        assert!(!path.exists());
    }

    #[test]
    fn format_log_prefix_adds_color_codes() {
        assert_eq!(format_log_prefix("host-broker", false), "[host-broker]");
        let colored = format_log_prefix("host-broker", true);
        assert!(colored.starts_with("\u{1b}[36m[host-broker]"));
        let serial = format_log_prefix("vm:foo:serial", true);
        assert!(serial.contains("\u{1b}[35m"));
        let default = format_log_prefix("vm:foo:qemu", true);
        assert!(default.contains("\u{1b}[34m"));
    }

    #[test]
    fn poll_log_sources_writes_new_lines() {
        let file = NamedTempFile::new().unwrap();
        writeln!(file.as_file(), "line1").unwrap();
        writeln!(file.as_file(), "").unwrap();
        let mut sources = [LogSource {
            prefix: "[unit]".into(),
            path: file.path().to_path_buf(),
            offset: 0,
        }];
        let activity = poll_log_sources(&mut sources).unwrap();
        assert!(activity);
        assert!(sources[0].offset > 0);
    }

    #[test]
    fn poll_log_sources_without_updates_returns_false() {
        let file = NamedTempFile::new().unwrap();
        let mut sources = [LogSource {
            prefix: "[unit]".into(),
            path: file.path().to_path_buf(),
            offset: 0,
        }];
        let activity = poll_log_sources(&mut sources).unwrap();
        assert!(!activity);
    }
}
