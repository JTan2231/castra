use std::collections::VecDeque;
use std::fs;
use std::io::{self, BufRead, Read, Write};
use std::net::TcpStream;
use std::path::Path;
use std::time::Duration;

use crate::error::{Error, Result};

use super::{Reporter, load_project_for_operation};
use crate::core::options::{BusLogTarget, BusPublishOptions, BusTailOptions};
use crate::core::outcome::{
    BusPublishOutcome, BusTailOutcome, LogEntry, LogFollower, LogSectionState, OperationOutput,
    OperationResult,
};
use crate::core::project::config_state_root;
use serde_json::Value;

const BUS_MAX_FRAME_SIZE: usize = 64 * 1024;
const HOST_HANDSHAKE_IDENTITY: &str = "host";
const HOST_HANDSHAKE_CAPABILITY: &str = "host-bus";

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
    let shared_path = bus_dir.join("bus.log");
    publish_via_broker(project.broker.port, &topic, &payload)?;

    let outcome = BusPublishOutcome {
        log_path: shared_path,
        topic,
    };

    Ok(OperationOutput::new(outcome).with_diagnostics(diagnostics))
}

fn publish_via_broker(port: u16, topic: &str, payload: &Value) -> Result<()> {
    let addr = format!("127.0.0.1:{port}");
    let mut stream = TcpStream::connect(&addr).map_err(|err| Error::BusPublishFailed {
        message: format!("Failed to connect to broker at {addr}: {err}"),
    })?;
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .map_err(|err| Error::BusPublishFailed {
            message: format!("Failed to configure broker socket timeout: {err}"),
        })?;
    stream
        .set_write_timeout(Some(Duration::from_secs(5)))
        .map_err(|err| Error::BusPublishFailed {
            message: format!("Failed to configure broker socket timeout: {err}"),
        })?;

    // Read and ignore the broker greeting.
    let _ = read_line(&mut stream).map_err(|err| Error::BusPublishFailed {
        message: format!("Broker greeting failed: {err}"),
    })?;

    let handshake = format!(
        "hello vm:{identity} capabilities=bus-v1,{capability}\n",
        identity = HOST_HANDSHAKE_IDENTITY,
        capability = HOST_HANDSHAKE_CAPABILITY
    );
    stream
        .write_all(handshake.as_bytes())
        .map_err(|err| Error::BusPublishFailed {
            message: format!("Failed to send broker handshake: {err}"),
        })?;
    stream.flush().map_err(|err| Error::BusPublishFailed {
        message: format!("Failed to flush broker handshake: {err}"),
    })?;

    let response = read_line(&mut stream).map_err(|err| Error::BusPublishFailed {
        message: format!("Failed to read broker handshake response: {err}"),
    })?;
    if !response.starts_with("ok") {
        return Err(Error::BusPublishFailed {
            message: format!("Broker rejected handshake: {response}"),
        });
    }

    let frame = serde_json::json!({
        "type": "publish",
        "topic": topic,
        "payload": payload,
    });
    let encoded = serde_json::to_vec(&frame).map_err(|err| Error::BusPublishFailed {
        message: format!("Failed to encode bus payload: {err}"),
    })?;
    if encoded.len() > BUS_MAX_FRAME_SIZE {
        return Err(Error::BusPublishFailed {
            message: format!(
                "Encoded frame length {} exceeds broker maximum {} bytes.",
                encoded.len(),
                BUS_MAX_FRAME_SIZE
            ),
        });
    }

    let len = (encoded.len() as u32).to_be_bytes();
    stream
        .write_all(&len)
        .map_err(|err| Error::BusPublishFailed {
            message: format!("Failed to send frame header to broker: {err}"),
        })?;
    stream
        .write_all(&encoded)
        .map_err(|err| Error::BusPublishFailed {
            message: format!("Failed to send frame payload to broker: {err}"),
        })?;
    stream.flush().map_err(|err| Error::BusPublishFailed {
        message: format!("Failed to flush frame to broker: {err}"),
    })?;

    read_publish_ack(&mut stream)?;
    Ok(())
}

fn read_line(stream: &mut TcpStream) -> io::Result<String> {
    let mut buffer = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        match stream.read(&mut byte) {
            Ok(0) => {
                if buffer.is_empty() {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "connection closed before newline",
                    ));
                }
                break;
            }
            Ok(_) => {
                if byte[0] == b'\n' {
                    break;
                }
                buffer.push(byte[0]);
            }
            Err(ref err) if err.kind() == io::ErrorKind::Interrupted => continue,
            Err(err) => return Err(err),
        }
    }

    Ok(String::from_utf8_lossy(&buffer).trim().to_string())
}

fn read_publish_ack(stream: &mut TcpStream) -> Result<()> {
    let mut len_buf = [0u8; 4];
    stream
        .read_exact(&mut len_buf)
        .map_err(|err| Error::BusPublishFailed {
            message: format!("Failed to read broker acknowledgement header: {err}"),
        })?;
    let frame_len = u32::from_be_bytes(len_buf) as usize;
    if frame_len == 0 || frame_len > BUS_MAX_FRAME_SIZE {
        return Err(Error::BusPublishFailed {
            message: format!(
                "Broker acknowledgement length {frame_len} exceeds limit {BUS_MAX_FRAME_SIZE}."
            ),
        });
    }

    let mut payload = vec![0u8; frame_len];
    stream
        .read_exact(&mut payload)
        .map_err(|err| Error::BusPublishFailed {
            message: format!("Failed to read broker acknowledgement payload: {err}"),
        })?;

    let value: Value = serde_json::from_slice(&payload).map_err(|err| Error::BusPublishFailed {
        message: format!("Failed to decode broker acknowledgement: {err}"),
    })?;
    let ack_type = value.get("type").and_then(Value::as_str).unwrap_or("?");
    if ack_type != "ack" {
        return Err(Error::BusPublishFailed {
            message: format!("Unexpected broker response: {value}"),
        });
    }

    let ack_kind = value.get("ack").and_then(Value::as_str).unwrap_or("?");
    if ack_kind != "publish" {
        return Err(Error::BusPublishFailed {
            message: format!("Unexpected broker ack `{ack_kind}`."),
        });
    }

    let status = value
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("error");
    if !status.eq_ignore_ascii_case("ok") {
        let reason = value
            .get("reason")
            .and_then(Value::as_str)
            .unwrap_or("unknown error");
        return Err(Error::BusPublishFailed {
            message: format!("Broker rejected publish: {reason}"),
        });
    }

    Ok(())
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
