use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::{self, ErrorKind, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::options::BrokerOptions;
use crate::error::{Error, Result};
use libc;
use serde::{Deserialize, Serialize};
use serde_json::{self, Value};

const BUS_MAX_FRAME_SIZE: usize = 64 * 1024;

static SESSION_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Debug)]
enum HandshakeError {
    Io(io::Error),
    Protocol(String),
    Storage(String),
}

#[derive(Debug)]
struct HandshakeDetails {
    vm: String,
    capabilities: Vec<String>,
}

impl HandshakeDetails {
    fn has_capability(&self, capability: &str) -> bool {
        self.capabilities
            .iter()
            .any(|value| value.eq_ignore_ascii_case(capability))
    }

    fn capability_note(&self) -> String {
        if self.capabilities.is_empty() {
            String::new()
        } else {
            format!(" capabilities={}", self.capabilities.join(","))
        }
    }
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

    let log_file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&options.logfile)
        .map_err(|err| Error::PreflightFailed {
            message: format!(
                "Unable to open broker log {}: {err}",
                options.logfile.display()
            ),
        })?;
    let log = Arc::new(Mutex::new(log_file));

    let bus_log_dir = options
        .logfile
        .parent()
        .map(|parent| parent.join("bus"))
        .unwrap_or_else(|| PathBuf::from("bus"));
    fs::create_dir_all(&bus_log_dir).map_err(|err| Error::PreflightFailed {
        message: format!(
            "Failed to prepare broker bus log directory {}: {err}",
            bus_log_dir.display()
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
        &log,
        "INFO",
        &format!("listening on 127.0.0.1:{}", options.port),
    )?;
    broker_log_line(
        &log,
        "INFO",
        &format!(
            "recording guest handshakes under {}",
            options.handshake_dir.display()
        ),
    )?;

    loop {
        match listener.accept() {
            Ok((mut stream, addr)) => {
                broker_log_line(&log, "INFO", &format!("connection from {addr}"))?;
                if let Err(err) = stream.set_read_timeout(Some(Duration::from_secs(5))) {
                    broker_log_line(
                        &log,
                        "WARN",
                        &format!("handshake setup failed (read timeout) for {addr}: {err}"),
                    )?;
                    continue;
                }
                if let Err(err) = stream.set_write_timeout(Some(Duration::from_secs(5))) {
                    broker_log_line(
                        &log,
                        "WARN",
                        &format!("handshake setup failed (write timeout) for {addr}: {err}"),
                    )?;
                    continue;
                }
                if let Err(err) = stream.write_all(b"castra-broker 0.1 ready\n") {
                    broker_log_line(
                        &log,
                        "WARN",
                        &format!("failed to send greeting to {addr}: {err}"),
                    )?;
                    continue;
                }
                match process_handshake(&mut stream, options.handshake_dir.as_path()) {
                    Ok(details) => {
                        let vm_name = details.vm.clone();
                        broker_log_line(
                            &log,
                            "INFO",
                            &format!(
                                "handshake success from {addr}: vm={vm}{caps}",
                                vm = vm_name,
                                caps = details.capability_note()
                            ),
                        )?;
                        if details.has_capability("bus-v1") {
                            let session = generate_session_token();
                            if let Err(err) =
                                stream.write_all(format!("ok session={session}\n").as_bytes())
                            {
                                broker_log_line(
                                    &log,
                                    "WARN",
                                    &format!(
                                        "failed to acknowledge bus handshake for {addr}: {err}"
                                    ),
                                )?;
                                continue;
                            }
                            if let Err(err) = stream.flush() {
                                broker_log_line(
                                    &log,
                                    "WARN",
                                    &format!("failed to flush bus handshake for {addr}: {err}"),
                                )?;
                                continue;
                            }

                            let logger = log.clone();
                            let session_vm = vm_name.clone();
                            let bus_dir = bus_log_dir.clone();
                            thread::spawn(move || {
                                handle_bus_session(stream, session_vm, session, bus_dir, logger);
                            });
                            continue;
                        }

                        if let Err(err) = stream.write_all(b"ok\n") {
                            broker_log_line(
                                &log,
                                "WARN",
                                &format!("failed to acknowledge handshake for {addr}: {err}"),
                            )?;
                        }
                    }
                    Err(HandshakeError::Protocol(reason)) => {
                        broker_log_line(
                            &log,
                            "WARN",
                            &format!("handshake protocol error from {addr}: {reason}"),
                        )?;
                        let _ = stream.write_all(format!("error: {reason}\n").as_bytes());
                    }
                    Err(HandshakeError::Io(err)) => {
                        broker_log_line(
                            &log,
                            "WARN",
                            &format!("handshake IO error from {addr}: {err}"),
                        )?;
                    }
                    Err(HandshakeError::Storage(reason)) => {
                        broker_log_line(
                            &log,
                            "ERROR",
                            &format!("handshake persistence failed for {addr}: {reason}"),
                        )?;
                        let _ = stream.write_all(format!("error: {reason}\n").as_bytes());
                    }
                }
            }
            Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
            Err(err) => {
                broker_log_line(&log, "ERROR", &format!("accept failed: {err}"))?;
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

fn broker_log_line(log: &Arc<Mutex<fs::File>>, level: &str, message: &str) -> Result<()> {
    let mut file = log.lock().map_err(|_| Error::PreflightFailed {
        message: "Broker log mutex poisoned.".to_string(),
    })?;
    let line = format!("[host-broker] {} {} {}", broker_timestamp(), level, message);
    file.write_all(line.as_bytes())
        .map_err(|err| Error::PreflightFailed {
            message: format!("Failed to write broker log entry: {err}"),
        })?;
    file.write_all(b"\n")
        .map_err(|err| Error::PreflightFailed {
            message: format!("Failed to finalize broker log entry: {err}"),
        })?;
    file.flush().map_err(|err| Error::PreflightFailed {
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

fn process_handshake(
    stream: &mut TcpStream,
    handshake_dir: &Path,
) -> HandshakeResult<HandshakeDetails> {
    let line = read_handshake_line(stream).map_err(HandshakeError::Io)?;
    let details = parse_handshake_line(&line)?;
    persist_handshake(handshake_dir, &details.vm, &details.capabilities)
        .map_err(|err| HandshakeError::Storage(err.to_string()))?;
    Ok(details)
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

fn parse_handshake_line(line: &str) -> HandshakeResult<HandshakeDetails> {
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

    let mut capabilities: Vec<String> = Vec::new();
    for token in parts {
        let mut kv = token.splitn(2, '=');
        let Some(key) = kv.next() else {
            continue;
        };
        if !key.eq_ignore_ascii_case("capabilities") {
            continue;
        }
        if let Some(value) = kv.next() {
            for capability in value.split(',') {
                let trimmed = capability.trim();
                if trimmed.is_empty() {
                    continue;
                }
                if !capabilities
                    .iter()
                    .any(|existing| existing.eq_ignore_ascii_case(trimmed))
                {
                    capabilities.push(trimmed.to_string());
                }
            }
        }
    }

    Ok(HandshakeDetails {
        vm: vm.to_string(),
        capabilities,
    })
}

fn persist_handshake(handshake_dir: &Path, vm: &str, capabilities: &[String]) -> io::Result<()> {
    fs::create_dir_all(handshake_dir)?;
    let filename = format!("{}.json", sanitize_vm_name(vm));
    let path = handshake_dir.join(filename);
    let tmp = path.with_extension("json.tmp");
    let record = StoredHandshake {
        vm: vm.to_string(),
        timestamp: unix_timestamp_seconds(),
        capabilities: capabilities.to_vec(),
    };
    let payload = serde_json::to_vec_pretty(&record)
        .map_err(|err| io::Error::new(io::ErrorKind::Other, err))?;
    fs::write(&tmp, payload)?;
    fs::rename(&tmp, &path)?;
    Ok(())
}

#[derive(Serialize)]
struct StoredHandshake {
    vm: String,
    timestamp: u64,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    capabilities: Vec<String>,
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

type BusResult<T> = std::result::Result<T, BusError>;

fn generate_session_token() -> String {
    let counter = SESSION_COUNTER.fetch_add(1, Ordering::Relaxed);
    let timestamp = unix_timestamp_seconds();
    format!("sess-{timestamp}-{counter}")
}

fn handle_bus_session(
    mut stream: TcpStream,
    vm: String,
    session: String,
    bus_log_dir: PathBuf,
    logger: Arc<Mutex<fs::File>>,
) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(30)));
    if let Ok(addr) = stream.peer_addr() {
        let _ = broker_log_line(
            &logger,
            "INFO",
            &format!(
                "bus session started for `{vm}` from {addr} (session={session})",
                vm = vm
            ),
        );
    } else {
        let _ = broker_log_line(
            &logger,
            "INFO",
            &format!("bus session started for `{vm}` (session={session})"),
        );
    }

    loop {
        match read_bus_frame(&mut stream) {
            Ok(frame) => match frame.kind.as_str() {
                "publish" => {
                    if let Err(err) = handle_publish(&vm, &frame, &bus_log_dir) {
                        let _ = broker_log_line(
                            &logger,
                            "WARN",
                            &format!("failed to persist bus publish from `{vm}`: {err}", vm = vm),
                        );
                        break;
                    }
                    let _ = broker_log_line(
                        &logger,
                        "INFO",
                        &format!(
                            "bus publish from `{vm}` topic={}",
                            frame.topic.as_deref().unwrap_or("broadcast")
                        ),
                    );
                }
                "heartbeat" => {
                    // Heartbeats keep the session fresh; avoid log spam but confirm trace on debug severity.
                }
                "subscribe" => {
                    let _ = broker_log_line(
                        &logger,
                        "INFO",
                        &format!(
                            "`{vm}` requested bus subscription to {:?} (not implemented)",
                            frame.topic
                        ),
                    );
                }
                other => {
                    let _ = broker_log_line(
                        &logger,
                        "WARN",
                        &format!("received unsupported bus frame `{other}` from `{vm}`"),
                    );
                }
            },
            Err(BusError::Timeout) => continue,
            Err(BusError::Io(err)) => {
                let _ = broker_log_line(
                    &logger,
                    "WARN",
                    &format!("bus session for `{vm}` ended due to IO error: {err}"),
                );
                break;
            }
            Err(BusError::Protocol(reason)) => {
                let _ = broker_log_line(
                    &logger,
                    "WARN",
                    &format!("bus session for `{vm}` ended due to protocol error: {reason}"),
                );
                let _ = stream.write_all(format!("error: {reason}\n").as_bytes());
                break;
            }
        }
    }

    let _ = broker_log_line(
        &logger,
        "INFO",
        &format!("bus session closed for `{vm}` (session={session})"),
    );
}

fn handle_publish(vm: &str, frame: &BusFrame, bus_log_dir: &Path) -> BusResult<()> {
    let topic = frame.topic.as_deref().unwrap_or("broadcast");
    let entry = serde_json::json!({
        "timestamp": unix_timestamp_seconds(),
        "vm": vm,
        "topic": topic,
        "payload": frame.payload.clone(),
    });
    let line = serde_json::to_string(&entry).map_err(|err| BusError::Protocol(err.to_string()))?;
    append_bus_log(bus_log_dir, vm, &line)?;
    append_shared_bus_log(bus_log_dir, &line)?;
    Ok(())
}

fn append_bus_log(dir: &Path, vm: &str, line: &str) -> BusResult<()> {
    fs::create_dir_all(dir).map_err(BusError::Io)?;
    let path = dir.join(format!("{}.log", sanitize_vm_name(vm)));
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(BusError::Io)?;
    file.write_all(line.as_bytes()).map_err(BusError::Io)?;
    file.write_all(b"\n").map_err(BusError::Io)?;
    Ok(())
}

fn append_shared_bus_log(dir: &Path, line: &str) -> BusResult<()> {
    fs::create_dir_all(dir).map_err(BusError::Io)?;
    let path = dir.join("bus.log");
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(BusError::Io)?;
    file.write_all(line.as_bytes()).map_err(BusError::Io)?;
    file.write_all(b"\n").map_err(BusError::Io)?;
    Ok(())
}

fn read_bus_frame(stream: &mut TcpStream) -> BusResult<BusFrame> {
    let mut len_buf = [0u8; 4];
    match stream.read_exact(&mut len_buf) {
        Ok(()) => {}
        Err(err) if err.kind() == ErrorKind::WouldBlock || err.kind() == ErrorKind::TimedOut => {
            return Err(BusError::Timeout);
        }
        Err(err) => return Err(BusError::Io(err)),
    }

    let frame_len = u32::from_be_bytes(len_buf) as usize;
    if frame_len == 0 || frame_len > BUS_MAX_FRAME_SIZE {
        return Err(BusError::Protocol(format!(
            "invalid frame length {frame_len} (max {BUS_MAX_FRAME_SIZE})"
        )));
    }

    let mut payload = vec![0u8; frame_len];
    match stream.read_exact(&mut payload) {
        Ok(()) => {}
        Err(err) if err.kind() == ErrorKind::WouldBlock || err.kind() == ErrorKind::TimedOut => {
            return Err(BusError::Timeout);
        }
        Err(err) => return Err(BusError::Io(err)),
    }

    let frame: BusFrame =
        serde_json::from_slice(&payload).map_err(|err| BusError::Protocol(err.to_string()))?;
    Ok(frame)
}

#[derive(Debug)]
enum BusError {
    Timeout,
    Io(io::Error),
    Protocol(String),
}

impl fmt::Display for BusError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BusError::Timeout => write!(f, "timeout"),
            BusError::Io(err) => write!(f, "io error: {err}"),
            BusError::Protocol(reason) => write!(f, "protocol error: {reason}"),
        }
    }
}

#[derive(Debug, Deserialize)]
struct BusFrame {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    topic: Option<String>,
    #[serde(default = "default_payload")]
    payload: Value,
}

fn default_payload() -> Value {
    Value::Null
}

#[cfg(test)]
mod tests {
    use super::*;
    use regex::Regex;
    use serde_json::json;
    use std::sync::{Arc, Mutex};
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
        let file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .unwrap();
        let logger = Arc::new(Mutex::new(file));
        broker_log_line(&logger, "INFO", "test message").unwrap();
        logger.lock().unwrap().flush().unwrap();
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

    #[test]
    fn parse_handshake_supports_capabilities() {
        let details = parse_handshake_line("hello vm:dev capabilities=bus-v1,bus-v1,metrics")
            .expect("handshake parse");
        assert_eq!(details.vm, "dev");
        assert_eq!(details.capabilities.len(), 2);
        assert!(details.capabilities.iter().any(|cap| cap == "bus-v1"));
        assert!(details.capabilities.iter().any(|cap| cap == "metrics"));
    }

    #[test]
    fn handle_publish_appends_bus_logs() {
        let dir = tempdir().unwrap();
        let frame = BusFrame {
            kind: "publish".to_string(),
            topic: Some("broadcast".to_string()),
            payload: json!({ "message": "hello" }),
        };
        handle_publish("devbox", &frame, dir.path()).expect("publish");
        let vm_log = dir.path().join("devbox.log");
        assert!(vm_log.exists());
        let contents = fs::read_to_string(&vm_log).unwrap();
        assert!(contents.contains("\"vm\":\"devbox\""));
        let shared = fs::read_to_string(dir.path().join("bus.log")).unwrap();
        assert!(shared.contains("\"topic\":\"broadcast\""));
    }
}
