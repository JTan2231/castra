use std::collections::VecDeque;
use std::fs;
use std::io::{self, BufRead, BufReader};
use std::path::{Path, PathBuf};

use crate::config::ProjectConfig;
use crate::error::{Error, Result};

use super::outcome::{LogEntry, LogFollower, LogSection, LogSectionState, LogsOutcome};
use super::project::config_state_root;

pub fn collect_logs(project: &ProjectConfig, tail: usize, follow: bool) -> Result<LogsOutcome> {
    let log_dir = config_state_root(project).join("logs");

    let mut sections = Vec::new();
    let mut follower_sources = Vec::new();

    let mut declared_sections: Vec<(String, PathBuf)> = Vec::new();
    declared_sections.push(("host-broker".to_string(), log_dir.join("broker.log")));

    for vm in &project.vms {
        declared_sections.push((
            format!("vm:{}:qemu", vm.name),
            log_dir.join(format!("{}.log", vm.name)),
        ));
        declared_sections.push((
            format!("vm:{}:serial", vm.name),
            log_dir.join(format!("{}-serial.log", vm.name)),
        ));
    }

    for (label, path) in declared_sections {
        let (entries, state, offset) = gather_tail(&path, tail)?;
        sections.push(LogSection {
            label: label.clone(),
            path: path.clone(),
            entries,
            state,
        });
        follower_sources.push((label, path, offset));
    }

    let follower = if follow {
        Some(LogFollower::from_sources(follower_sources))
    } else {
        None
    };

    Ok(LogsOutcome { sections, follower })
}

fn gather_tail(path: &Path, tail: usize) -> Result<(Vec<LogEntry>, LogSectionState, u64)> {
    if !path.exists() {
        return Ok((Vec::new(), LogSectionState::NotCreated, 0));
    }

    let entries = if tail > 0 {
        match read_tail_lines(path, tail) {
            Ok(lines) => lines
                .into_iter()
                .map(|line| LogEntry {
                    line: if line.is_empty() { None } else { Some(line) },
                })
                .collect(),
            Err(err) => {
                if err.kind() == io::ErrorKind::NotFound {
                    Vec::new()
                } else {
                    return Err(Error::LogReadFailed {
                        path: path.to_path_buf(),
                        source: err,
                    });
                }
            }
        }
    } else {
        Vec::new()
    };

    let offset = fs::metadata(path).map(|meta| meta.len()).unwrap_or(0);
    let state = if offset == 0 {
        LogSectionState::Empty
    } else if entries.is_empty() {
        LogSectionState::HasEntries
    } else {
        LogSectionState::HasEntries
    };

    Ok((entries, state, offset))
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
