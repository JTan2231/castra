use std::collections::VecDeque;
use std::fs::{self, OpenOptions};
use std::io::{self, BufRead, Write};
use std::path::Path;

use crate::error::{Error, Result};

use super::{Reporter, load_project_for_operation};
use crate::core::options::{BusLogTarget, BusPublishOptions, BusTailOptions};
use crate::core::outcome::{
    BusPublishOutcome, BusTailOutcome, LogEntry, LogFollower, LogSectionState, OperationOutput,
    OperationResult,
};
use crate::core::project::config_state_root;

pub fn publish(
    options: BusPublishOptions,
    _reporter: Option<&mut dyn Reporter>,
) -> OperationResult<BusPublishOutcome> {
    let mut diagnostics = Vec::new();
    let BusPublishOptions {
        config,
        topic,
        payload,
    } = options;

    let (project, _) = load_project_for_operation(&config, &mut diagnostics)?;
    let state_root = config_state_root(&project);
    let bus_dir = state_root.join("logs").join("bus");
    fs::create_dir_all(&bus_dir).map_err(|err| Error::PreflightFailed {
        message: format!(
            "Failed to prepare bus log directory {}: {err}",
            bus_dir.display()
        ),
    })?;

    let entry = serde_json::json!({
        "timestamp": timestamp_seconds(),
        "vm": "host",
        "topic": topic.clone(),
        "payload": payload,
    });

    let line = serde_json::to_string(&entry).map_err(|err| Error::PreflightFailed {
        message: format!("Failed to encode bus payload: {err}"),
    })?;

    let shared_path = bus_dir.join("bus.log");
    append_line(&shared_path, &line)?;

    if let Some(target_vm) = topic.strip_prefix("vm:") {
        let vm_path = bus_dir.join(format!("{}.log", sanitize_vm_name(target_vm)));
        append_line(&vm_path, &line)?;
    }

    let outcome = BusPublishOutcome {
        log_path: shared_path,
        topic,
    };

    Ok(OperationOutput::new(outcome).with_diagnostics(diagnostics))
}

pub fn tail(
    options: BusTailOptions,
    _reporter: Option<&mut dyn Reporter>,
) -> OperationResult<BusTailOutcome> {
    let mut diagnostics = Vec::new();
    let BusTailOptions {
        config,
        target,
        tail,
        follow,
    } = options;

    let (project, _) = load_project_for_operation(&config, &mut diagnostics)?;
    let state_root = config_state_root(&project);
    let bus_dir = state_root.join("logs").join("bus");

    let (log_label, log_path) = match &target {
        BusLogTarget::Shared => ("bus".to_string(), bus_dir.join("bus.log")),
        BusLogTarget::Vm(name) => (
            format!("bus:{}", name),
            bus_dir.join(format!("{}.log", sanitize_vm_name(name))),
        ),
    };

    let (entries, state, offset) = gather_bus_tail(&log_path, tail)?;
    let follower = if follow {
        Some(LogFollower::from_sources(vec![(
            log_label.clone(),
            log_path.clone(),
            offset,
        )]))
    } else {
        None
    };

    let outcome = BusTailOutcome {
        project_path: project.file_path.clone(),
        project_name: project.project_name.clone(),
        target,
        log_label,
        log_path,
        entries,
        state,
        follower,
    };

    Ok(OperationOutput::new(outcome).with_diagnostics(diagnostics))
}

fn append_line(path: &Path, line: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| Error::PreflightFailed {
            message: format!(
                "Failed to prepare bus log directory {}: {err}",
                parent.display()
            ),
        })?;
    }

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|err| Error::PreflightFailed {
            message: format!("Unable to open bus log {}: {err}", path.display()),
        })?;
    file.write_all(line.as_bytes())
        .map_err(|err| Error::PreflightFailed {
            message: format!("Unable to write bus log {}: {err}", path.display()),
        })?;
    file.write_all(b"\n")
        .map_err(|err| Error::PreflightFailed {
            message: format!("Unable to finalize bus log {}: {err}", path.display()),
        })?;
    Ok(())
}

fn gather_bus_tail(path: &Path, tail: usize) -> Result<(Vec<LogEntry>, LogSectionState, u64)> {
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
    let reader = io::BufReader::new(file);
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

fn sanitize_vm_name(name: &str) -> String {
    let mut sanitized: String = name
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '_'
            }
        })
        .collect();
    if sanitized.chars().all(|ch| ch == '_' || ch == '.') {
        sanitized = "vm".to_string();
    }
    sanitized
}

fn timestamp_seconds() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};

    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}
