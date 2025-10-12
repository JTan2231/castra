use std::fs;
use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::options::BrokerOptions;
use crate::error::{Error, Result};
use libc;
use serde::Serialize;

#[derive(Debug)]
enum HandshakeError {
    Io(io::Error),
    Protocol(String),
    Storage(String),
}

pub fn run(options: &BrokerOptions) -> Result<()> {
    if let Some(parent) = options.pidfile.parent() {
        fs::create_dir_all(parent).map_err(|err| Error::PreflightFailed {
            message: format!(
                "Failed to prepare broker pidfile directory {}: {err}",
                parent.display()
            ),
        })?;
    }
    if let Some(parent) = options.logfile.parent() {
        fs::create_dir_all(parent).map_err(|err| Error::PreflightFailed {
            message: format!(
                "Failed to prepare broker log directory {}: {err}",
                parent.display()
            ),
        })?;
    }
    fs::create_dir_all(&options.handshake_dir).map_err(|err| Error::PreflightFailed {
        message: format!(
            "Failed to prepare broker handshake directory {}: {err}",
            options.handshake_dir.display()
        ),
    })?;

    let listener =
        TcpListener::bind(("127.0.0.1", options.port)).map_err(|err| Error::PreflightFailed {
            message: format!("Broker failed to bind 127.0.0.1:{}: {err}", options.port),
        })?;

    let mut log = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&options.logfile)
        .map_err(|err| Error::PreflightFailed {
            message: format!(
                "Unable to open broker log {}: {err}",
                options.logfile.display()
            ),
        })?;

    fs::write(&options.pidfile, format!("{}\n", std::process::id())).map_err(|err| {
        Error::PreflightFailed {
            message: format!(
                "Failed to write broker pidfile {}: {err}",
                options.pidfile.display()
            ),
        }
    })?;
    let _guard = PidfileGuard {
        path: options.pidfile.clone(),
    };

    broker_log_line(
        &mut log,
        "INFO",
        &format!("listening on 127.0.0.1:{}", options.port),
    )?;
    broker_log_line(
        &mut log,
        "INFO",
        &format!(
            "recording guest handshakes under {}",
            options.handshake_dir.display()
        ),
    )?;

    loop {
        match listener.accept() {
            Ok((mut stream, addr)) => {
                broker_log_line(&mut log, "INFO", &format!("connection from {addr}"))?;
                if let Err(err) = stream.set_read_timeout(Some(Duration::from_secs(5))) {
                    broker_log_line(
                        &mut log,
                        "WARN",
                        &format!("handshake setup failed (read timeout) for {addr}: {err}"),
                    )?;
                    continue;
                }
                if let Err(err) = stream.set_write_timeout(Some(Duration::from_secs(5))) {
                    broker_log_line(
                        &mut log,
                        "WARN",
                        &format!("handshake setup failed (write timeout) for {addr}: {err}"),
                    )?;
                    continue;
                }
                if let Err(err) = stream.write_all(b"castra-broker 0.1 ready\n") {
                    broker_log_line(
                        &mut log,
                        "WARN",
                        &format!("failed to send greeting to {addr}: {err}"),
                    )?;
                    continue;
                }
                match process_handshake(&mut stream, options.handshake_dir.as_path()) {
                    Ok(vm) => {
                        broker_log_line(
                            &mut log,
                            "INFO",
                            &format!("handshake success from {addr}: vm={vm}"),
                        )?;
                        if let Err(err) = stream.write_all(b"ok\n") {
                            broker_log_line(
                                &mut log,
                                "WARN",
                                &format!("failed to acknowledge handshake for {addr}: {err}"),
                            )?;
                        }
                    }
                    Err(HandshakeError::Protocol(reason)) => {
                        broker_log_line(
                            &mut log,
                            "WARN",
                            &format!("handshake protocol error from {addr}: {reason}"),
                        )?;
                        let _ = stream.write_all(format!("error: {reason}\n").as_bytes());
                    }
                    Err(HandshakeError::Io(err)) => {
                        broker_log_line(
                            &mut log,
                            "WARN",
                            &format!("handshake IO error from {addr}: {err}"),
                        )?;
                    }
                    Err(HandshakeError::Storage(reason)) => {
                        broker_log_line(
                            &mut log,
                            "ERROR",
                            &format!("handshake persistence failed for {addr}: {reason}"),
                        )?;
                        let _ = stream.write_all(format!("error: {reason}\n").as_bytes());
                    }
                }
            }
            Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
            Err(err) => {
                broker_log_line(&mut log, "ERROR", &format!("accept failed: {err}"))?;
                thread::sleep(Duration::from_millis(200));
            }
        }
    }
}

struct PidfileGuard {
    path: PathBuf,
}

impl Drop for PidfileGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn broker_log_line(log: &mut fs::File, level: &str, message: &str) -> Result<()> {
    let line = format!("[host-broker] {} {} {}", broker_timestamp(), level, message);
    log.write_all(line.as_bytes())
        .map_err(|err| Error::PreflightFailed {
            message: format!("Failed to write broker log entry: {err}"),
        })?;
    log.write_all(b"\n").map_err(|err| Error::PreflightFailed {
        message: format!("Failed to finalize broker log entry: {err}"),
    })?;
    log.flush().map_err(|err| Error::PreflightFailed {
        message: format!("Failed to flush broker log: {err}"),
    })?;
    Ok(())
}

fn broker_timestamp() -> String {
    let now = SystemTime::now();
    let Ok(duration) = now.duration_since(UNIX_EPOCH) else {
        return "00:00:00".to_string();
    };
    let mut raw: libc::time_t = duration.as_secs() as libc::time_t;
    let mut tm = unsafe { std::mem::zeroed::<libc::tm>() };
    let ptr = unsafe { libc::localtime_r(&mut raw, &mut tm) };
    if ptr.is_null() {
        return "00:00:00".to_string();
    }
    format!("{:02}:{:02}:{:02}", tm.tm_hour, tm.tm_min, tm.tm_sec)
}

type HandshakeResult<T> = std::result::Result<T, HandshakeError>;

fn process_handshake(stream: &mut TcpStream, handshake_dir: &Path) -> HandshakeResult<String> {
    let line = read_handshake_line(stream).map_err(HandshakeError::Io)?;
    let vm = parse_handshake_line(&line)?;
    persist_handshake(handshake_dir, &vm)
        .map_err(|err| HandshakeError::Storage(err.to_string()))?;
    Ok(vm)
}

fn read_handshake_line(stream: &mut TcpStream) -> io::Result<String> {
    let mut buffer = Vec::with_capacity(128);
    let mut byte = [0u8; 1];
    while buffer.len() < 512 {
        let read = stream.read(&mut byte)?;
        if read == 0 {
            break;
        }
        if byte[0] == b'\n' {
            break;
        }
        buffer.push(byte[0]);
    }

    if buffer.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "connection closed before handshake",
        ));
    }
    if buffer.len() >= 512 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "handshake line exceeded 512 bytes",
        ));
    }

    let line = String::from_utf8_lossy(&buffer);
    Ok(line.trim().to_string())
}

fn parse_handshake_line(line: &str) -> HandshakeResult<String> {
    if line.is_empty() {
        return Err(HandshakeError::Protocol(
            "empty handshake payload".to_string(),
        ));
    }

    let mut parts = line.split_whitespace();
    let Some(keyword) = parts.next() else {
        return Err(HandshakeError::Protocol(
            "handshake missing keyword".to_string(),
        ));
    };
    if !keyword.eq_ignore_ascii_case("hello") {
        return Err(HandshakeError::Protocol(format!(
            "unexpected handshake keyword `{keyword}`"
        )));
    }

    let Some(identity) = parts.next() else {
        return Err(HandshakeError::Protocol(
            "handshake missing identity".to_string(),
        ));
    };

    let vm = identity.strip_prefix("vm:").unwrap_or(identity).trim();
    if vm.is_empty() {
        return Err(HandshakeError::Protocol(
            "handshake identity must not be empty".to_string(),
        ));
    }
    if vm.len() > 128 {
        return Err(HandshakeError::Protocol(
            "handshake identity exceeds 128 characters".to_string(),
        ));
    }

    Ok(vm.to_string())
}

fn persist_handshake(handshake_dir: &Path, vm: &str) -> io::Result<()> {
    fs::create_dir_all(handshake_dir)?;
    let filename = format!("{}.json", sanitize_vm_name(vm));
    let path = handshake_dir.join(filename);
    let tmp = path.with_extension("json.tmp");
    let record = StoredHandshake {
        vm,
        timestamp: unix_timestamp_seconds(),
    };
    let payload = serde_json::to_vec_pretty(&record)
        .map_err(|err| io::Error::new(io::ErrorKind::Other, err))?;
    fs::write(&tmp, payload)?;
    fs::rename(&tmp, &path)?;
    Ok(())
}

#[derive(Serialize)]
struct StoredHandshake<'a> {
    vm: &'a str,
    timestamp: u64,
}

fn unix_timestamp_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
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

#[cfg(test)]
mod tests {
    use super::*;
    use regex::Regex;
    use tempfile::tempdir;

    #[test]
    fn broker_timestamp_produces_hms_format() {
        let re = Regex::new(r"^\d{2}:\d{2}:\d{2}$").unwrap();
        assert!(re.is_match(&broker_timestamp()));
    }

    #[test]
    fn broker_log_line_writes_expected_format() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("broker.log");
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .unwrap();
        broker_log_line(&mut file, "INFO", "test message").unwrap();
        file.flush().unwrap();
        let contents = fs::read_to_string(&path).unwrap();
        assert!(contents.contains("[host-broker]"));
        assert!(contents.contains("INFO test message"));
    }

    #[test]
    fn pidfile_guard_removes_file_on_drop() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("broker.pid");
        fs::write(&path, "123").unwrap();
        assert!(path.exists());
        {
            let _guard = PidfileGuard { path: path.clone() };
        }
        assert!(!path.exists());
    }
}
