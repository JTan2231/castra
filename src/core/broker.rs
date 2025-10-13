use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::{self, ErrorKind, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use super::options::BrokerOptions;
use crate::error::{Error, Result};
use libc;
use serde::{Deserialize, Serialize};
use serde_json::{self, Value};

const BUS_MAX_FRAME_SIZE: usize = 64 * 1024;
const BUS_HEARTBEAT_TIMEOUT: Duration = Duration::from_secs(60);
const BUS_MAX_SUBSCRIPTIONS: usize = 16;
const HANDSHAKE_EVENT_LOG: &str = "handshake-events.jsonl";

static SESSION_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SessionKind {
    Guest,
    Host,
}

impl SessionKind {
    fn as_str(self) -> &'static str {
        match self {
            SessionKind::Guest => "guest",
            SessionKind::Host => "host",
        }
    }
}

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
    kind: SessionKind,
}

#[derive(Debug)]
struct RecordedHandshake {
    details: HandshakeDetails,
    timestamp: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum HandshakeSessionOutcome {
    Granted,
    Denied { reason: String },
}

impl HandshakeSessionOutcome {
    fn granted() -> Self {
        Self::Granted
    }

    fn denied(reason: impl Into<String>) -> Self {
        Self::Denied {
            reason: reason.into(),
        }
    }

    fn is_granted(&self) -> bool {
        matches!(self, Self::Granted)
    }

    fn status(&self) -> &'static str {
        match self {
            Self::Granted => "granted",
            Self::Denied { .. } => "denied",
        }
    }

    fn reason(&self) -> Option<&str> {
        match self {
            Self::Granted => None,
            Self::Denied { reason } => Some(reason.as_str()),
        }
    }
}

#[derive(Debug, Serialize)]
struct BrokerHandshakeEventRecord {
    timestamp: u64,
    vm: String,
    capabilities: Vec<String>,
    session_kind: String,
    session_outcome: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    remote_addr: Option<String>,
}

impl BrokerHandshakeEventRecord {
    fn new(
        timestamp: u64,
        vm: &str,
        capabilities: &[String],
        session_kind: SessionKind,
        outcome: &HandshakeSessionOutcome,
        remote_addr: Option<&str>,
    ) -> Self {
        let mut caps: Vec<String> = capabilities.iter().cloned().collect();
        caps.sort_unstable_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));
        caps.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
        Self {
            timestamp,
            vm: vm.to_string(),
            capabilities: caps,
            session_kind: session_kind.as_str().to_string(),
            session_outcome: outcome.status().to_string(),
            reason: outcome.reason().map(|value| value.to_string()),
            remote_addr: remote_addr.map(|value| value.to_string()),
        }
    }
}

fn append_handshake_event(
    handshake_dir: &Path,
    record: &BrokerHandshakeEventRecord,
) -> io::Result<()> {
    fs::create_dir_all(handshake_dir)?;
    let path = handshake_dir.join(HANDSHAKE_EVENT_LOG);
    let mut file = OpenOptions::new().create(true).append(true).open(&path)?;
    serde_json::to_writer(&mut file, record).map_err(|err| {
        io::Error::new(
            io::ErrorKind::Other,
            format!("failed to encode handshake event: {err}"),
        )
    })?;
    file.write_all(b"\n")?;
    file.flush()?;
    Ok(())
}

fn handshake_log_message(
    timestamp: u64,
    vm: &str,
    remote: Option<std::net::SocketAddr>,
    capabilities: &[String],
    session_kind: SessionKind,
    outcome: &HandshakeSessionOutcome,
) -> String {
    let mut parts = Vec::with_capacity(6);
    parts.push(format!("handshake ts={timestamp}"));
    parts.push(format!("vm={vm}"));
    if let Some(addr) = remote {
        parts.push(format!("remote={addr}"));
    }
    let caps = if capabilities.is_empty() {
        "-".to_string()
    } else {
        capabilities.join(",")
    };
    parts.push(format!("capabilities=[{caps}]"));
    parts.push(format!("session_kind={}", session_kind.as_str()));
    parts.push(format!("session_outcome={}", outcome.status()));
    if let Some(reason) = outcome.reason() {
        parts.push(format!("reason={reason}"));
    }
    parts.join(" ")
}
impl HandshakeDetails {
    fn has_capability(&self, capability: &str) -> bool {
        self.capabilities
            .iter()
            .any(|value| value.eq_ignore_ascii_case(capability))
    }

    fn is_host(&self) -> bool {
        matches!(self.kind, SessionKind::Host)
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
                    Ok(recorded) => {
                        let timestamp = recorded.timestamp;
                        let details = recorded.details;
                        let vm_label = details.vm.clone();
                        let remote_display = Some(addr.to_string());
                        let session_outcome = if details.has_capability("bus-v1") {
                            HandshakeSessionOutcome::granted()
                        } else {
                            HandshakeSessionOutcome::denied("missing-capability")
                        };
                        let event_record = BrokerHandshakeEventRecord::new(
                            timestamp,
                            &vm_label,
                            &details.capabilities,
                            details.kind,
                            &session_outcome,
                            remote_display.as_deref(),
                        );
                        if let Err(err) =
                            append_handshake_event(&options.handshake_dir, &event_record)
                        {
                            broker_log_line(
                                &log,
                                "WARN",
                                &format!(
                                    "failed to record handshake event for `{}` from {addr}: {err}",
                                    vm_label
                                ),
                            )?;
                        }
                        let log_message = handshake_log_message(
                            timestamp,
                            &vm_label,
                            Some(addr),
                            &details.capabilities,
                            details.kind,
                            &session_outcome,
                        );
                        broker_log_line(&log, "INFO", &log_message)?;

                        if session_outcome.is_granted() {
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
                            let session_vm = vm_label.clone();
                            let bus_dir = bus_log_dir.clone();
                            let handshake_dir = options.handshake_dir.clone();
                            let session_kind = details.kind;
                            thread::spawn(move || {
                                handle_bus_session(
                                    stream,
                                    session_vm,
                                    session,
                                    bus_dir,
                                    handshake_dir,
                                    logger,
                                    session_kind,
                                );
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
) -> HandshakeResult<RecordedHandshake> {
    let line = read_handshake_line(stream).map_err(HandshakeError::Io)?;
    let details = parse_handshake_line(&line)?;
    let timestamp = unix_timestamp_seconds();
    if !details.is_host() {
        persist_handshake(handshake_dir, &details.vm, &details.capabilities, timestamp)
            .map_err(|err| HandshakeError::Storage(err.to_string()))?;
    }
    Ok(RecordedHandshake { details, timestamp })
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

    capabilities.sort_unstable_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));

    let kind = if capabilities
        .iter()
        .any(|cap| cap.eq_ignore_ascii_case("host-bus"))
    {
        SessionKind::Host
    } else {
        SessionKind::Guest
    };

    Ok(HandshakeDetails {
        vm: vm.to_string(),
        capabilities,
        kind,
    })
}

fn persist_handshake(
    handshake_dir: &Path,
    vm: &str,
    capabilities: &[String],
    timestamp: u64,
) -> io::Result<()> {
    let mut record = StoredHandshake {
        vm: vm.to_string(),
        timestamp,
        capabilities: capabilities.to_vec(),
        bus: None,
    };
    if capabilities
        .iter()
        .any(|cap| cap.eq_ignore_ascii_case("bus-v1"))
    {
        record.bus = Some(StoredBusState {
            protocol: Some("bus-v1".to_string()),
            ..StoredBusState::default()
        });
    }
    save_handshake_record(handshake_dir, &record)
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct StoredHandshake {
    vm: String,
    timestamp: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    capabilities: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    bus: Option<StoredBusState>,
}

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
struct StoredBusState {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    protocol: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    subscribed_topics: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    last_publish_ts: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    last_heartbeat_ts: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    last_subscribe_ts: Option<u64>,
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

fn handshake_record_path(handshake_dir: &Path, vm: &str) -> PathBuf {
    handshake_dir.join(format!("{}.json", sanitize_vm_name(vm)))
}

fn load_handshake_record(handshake_dir: &Path, vm: &str) -> io::Result<Option<StoredHandshake>> {
    let path = handshake_record_path(handshake_dir, vm);
    match fs::read(&path) {
        Ok(bytes) => {
            let record = serde_json::from_slice::<StoredHandshake>(&bytes).map_err(|err| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("invalid handshake JSON: {err}"),
                )
            })?;
            Ok(Some(record))
        }
        Err(err) if err.kind() == ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err),
    }
}

fn save_handshake_record(handshake_dir: &Path, record: &StoredHandshake) -> io::Result<()> {
    fs::create_dir_all(handshake_dir)?;
    let path = handshake_record_path(handshake_dir, &record.vm);
    let tmp = path.with_extension("json.tmp");
    let payload = serde_json::to_vec_pretty(record).map_err(|err| {
        io::Error::new(
            io::ErrorKind::Other,
            format!("failed to encode handshake: {err}"),
        )
    })?;
    fs::write(&tmp, payload)?;
    fs::rename(&tmp, &path)?;
    Ok(())
}

fn update_handshake_record<F>(
    handshake_dir: &Path,
    vm: &str,
    mut update: F,
) -> io::Result<StoredHandshake>
where
    F: FnMut(&mut StoredHandshake),
{
    let mut record = load_handshake_record(handshake_dir, vm)?.unwrap_or_else(|| StoredHandshake {
        vm: vm.to_string(),
        timestamp: unix_timestamp_seconds(),
        capabilities: Vec::new(),
        bus: None,
    });
    update(&mut record);
    save_handshake_record(handshake_dir, &record)?;
    Ok(record)
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
    handshake_dir: PathBuf,
    logger: Arc<Mutex<fs::File>>,
    session_kind: SessionKind,
) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let mut last_heartbeat = Instant::now();
    let mut subscribed_topics: Vec<String> = Vec::new();
    let track_handshake = matches!(session_kind, SessionKind::Guest);

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

    if track_handshake {
        if let Err(err) = update_handshake_record(&handshake_dir, &vm, |record| {
            let now = unix_timestamp_seconds();
            let bus = record.bus.get_or_insert_with(StoredBusState::default);
            if bus.protocol.is_none() {
                bus.protocol = Some("bus-v1".to_string());
            }
            bus.last_heartbeat_ts = Some(now);
            bus.subscribed_topics.clear();
            bus.last_subscribe_ts = None;
        }) {
            let _ = broker_log_line(
                &logger,
                "WARN",
                &format!("failed to initialize bus state for `{vm}`: {err}", vm = vm),
            );
        }
    }

    loop {
        match read_bus_frame(&mut stream) {
            Ok(frame) => match frame.kind.as_str() {
                "publish" => {
                    let topic_label = frame.topic.as_deref().unwrap_or("broadcast");
                    match handle_publish(&vm, &frame, &bus_log_dir) {
                        Ok(()) => {
                            if track_handshake {
                                if let Err(err) =
                                    update_handshake_record(&handshake_dir, &vm, |record| {
                                        let now = unix_timestamp_seconds();
                                        let bus =
                                            record.bus.get_or_insert_with(StoredBusState::default);
                                        bus.last_publish_ts = Some(now);
                                    })
                                {
                                    let _ = broker_log_line(
                                        &logger,
                                        "WARN",
                                        &format!(
                                            "failed to record publish timestamp for `{vm}`: {err}",
                                            vm = vm
                                        ),
                                    );
                                }
                            }
                            let _ = broker_log_line(
                                &logger,
                                "INFO",
                                &format!("bus publish from `{vm}` topic={topic_label}"),
                            );
                            if let Err(err) =
                                send_ack(&mut stream, "publish", frame.topic.as_deref(), "ok", None)
                            {
                                let _ = broker_log_line(
                                    &logger,
                                    "WARN",
                                    &format!(
                                        "failed to acknowledge publish for `{vm}`: {err}",
                                        vm = vm
                                    ),
                                );
                                break;
                            }
                        }
                        Err(err) => {
                            let reason = err.to_string();
                            let _ = broker_log_line(
                                &logger,
                                "WARN",
                                &format!(
                                    "failed to persist bus publish from `{vm}`: {reason}",
                                    vm = vm
                                ),
                            );
                            let _ = send_ack(
                                &mut stream,
                                "publish",
                                frame.topic.as_deref(),
                                "error",
                                Some(reason.as_str()),
                            );
                            break;
                        }
                    }
                }
                "heartbeat" => {
                    last_heartbeat = Instant::now();
                    if track_handshake {
                        if let Err(err) = update_handshake_record(&handshake_dir, &vm, |record| {
                            let now = unix_timestamp_seconds();
                            let bus = record.bus.get_or_insert_with(StoredBusState::default);
                            bus.last_heartbeat_ts = Some(now);
                        }) {
                            let _ = broker_log_line(
                                &logger,
                                "WARN",
                                &format!("failed to record heartbeat for `{vm}`: {err}", vm = vm),
                            );
                        }
                    }
                    if let Err(err) = send_ack(&mut stream, "heartbeat", None, "ok", None) {
                        let _ = broker_log_line(
                            &logger,
                            "WARN",
                            &format!("failed to acknowledge heartbeat for `{vm}`: {err}", vm = vm),
                        );
                        break;
                    }
                }
                "subscribe" => {
                    let topic = frame.topic.as_deref().map(str::trim);
                    let Some(topic) = topic.filter(|value| !value.is_empty()) else {
                        let _ = broker_log_line(
                            &logger,
                            "WARN",
                            &format!("`{vm}` sent subscribe without topic"),
                        );
                        let _ = send_ack(
                            &mut stream,
                            "subscribe",
                            None,
                            "error",
                            Some("missing topic"),
                        );
                        continue;
                    };
                    if !subscribed_topics
                        .iter()
                        .any(|existing| existing.eq_ignore_ascii_case(topic))
                    {
                        if subscribed_topics.len() >= BUS_MAX_SUBSCRIPTIONS {
                            let reason =
                                format!("subscription limit {BUS_MAX_SUBSCRIPTIONS} exceeded");
                            let _ = broker_log_line(
                                &logger,
                                "WARN",
                                &format!("`{vm}` exceeded subscription limit"),
                            );
                            let _ = send_ack(
                                &mut stream,
                                "subscribe",
                                Some(topic),
                                "error",
                                Some(reason.as_str()),
                            );
                            continue;
                        }
                        subscribed_topics.push(topic.to_string());
                        subscribed_topics
                            .sort_unstable_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));
                    }
                    if track_handshake {
                        if let Err(err) = update_handshake_record(&handshake_dir, &vm, |record| {
                            let now = unix_timestamp_seconds();
                            let bus = record.bus.get_or_insert_with(StoredBusState::default);
                            bus.subscribed_topics = subscribed_topics.clone();
                            bus.last_subscribe_ts = Some(now);
                        }) {
                            let _ = broker_log_line(
                                &logger,
                                "WARN",
                                &format!(
                                    "failed to record subscription for `{vm}`: {err}",
                                    vm = vm
                                ),
                            );
                        } else {
                            let _ = broker_log_line(
                                &logger,
                                "INFO",
                                &format!("`{vm}` subscribed to {:?}", subscribed_topics),
                            );
                        }
                    } else {
                        let _ = broker_log_line(
                            &logger,
                            "INFO",
                            &format!("`{vm}` subscribed to {:?}", subscribed_topics),
                        );
                    }
                    if let Err(err) = send_ack(&mut stream, "subscribe", Some(topic), "ok", None) {
                        let _ = broker_log_line(
                            &logger,
                            "WARN",
                            &format!("failed to acknowledge subscribe for `{vm}`: {err}", vm = vm),
                        );
                        break;
                    }
                }
                other => {
                    let _ = broker_log_line(
                        &logger,
                        "WARN",
                        &format!("received unsupported bus frame `{other}` from `{vm}`"),
                    );
                }
            },
            Err(BusError::Timeout) => {
                if last_heartbeat.elapsed() >= BUS_HEARTBEAT_TIMEOUT {
                    let _ = broker_log_line(
                        &logger,
                        "WARN",
                        &format!("bus session for `{vm}` timed out waiting for heartbeat"),
                    );
                    break;
                }
                continue;
            }
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

    if track_handshake {
        if let Err(err) = update_handshake_record(&handshake_dir, &vm, |record| {
            if let Some(bus) = record.bus.as_mut() {
                bus.subscribed_topics.clear();
                bus.last_subscribe_ts = None;
            }
        }) {
            let _ = broker_log_line(
                &logger,
                "WARN",
                &format!("failed to clear subscription state for `{vm}`: {err}"),
            );
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
    if let Some(target_vm) = frame
        .topic
        .as_deref()
        .and_then(|topic| topic.strip_prefix("vm:"))
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        if !target_vm.eq_ignore_ascii_case(vm) {
            append_bus_log(bus_log_dir, target_vm, &line)?;
        }
    }
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

fn send_bus_frame(stream: &mut TcpStream, value: &Value) -> BusResult<()> {
    let payload = serde_json::to_vec(value).map_err(|err| BusError::Protocol(err.to_string()))?;
    if payload.len() > BUS_MAX_FRAME_SIZE {
        return Err(BusError::Protocol(format!(
            "outgoing frame length {} exceeds max {}",
            payload.len(),
            BUS_MAX_FRAME_SIZE
        )));
    }
    let len = (payload.len() as u32).to_be_bytes();
    stream.write_all(&len).map_err(BusError::Io)?;
    stream.write_all(&payload).map_err(BusError::Io)?;
    stream.flush().map_err(BusError::Io)?;
    Ok(())
}

fn send_ack(
    stream: &mut TcpStream,
    ack_kind: &str,
    topic: Option<&str>,
    status: &str,
    reason: Option<&str>,
) -> BusResult<()> {
    let mut map = serde_json::Map::new();
    map.insert("type".to_string(), Value::String("ack".to_string()));
    map.insert("ack".to_string(), Value::String(ack_kind.to_string()));
    map.insert("status".to_string(), Value::String(status.to_string()));
    if let Some(topic) = topic {
        map.insert("topic".to_string(), Value::String(topic.to_string()));
    }
    if let Some(reason) = reason {
        map.insert("reason".to_string(), Value::String(reason.to_string()));
    }
    send_bus_frame(stream, &Value::Object(map))
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
        assert!(matches!(details.kind, SessionKind::Guest));
    }

    #[test]
    fn parse_handshake_marks_host_sessions() {
        let details = parse_handshake_line("hello vm:host capabilities=bus-v1,HOST-BUS").unwrap();
        assert_eq!(details.vm, "host");
        assert!(
            details
                .capabilities
                .iter()
                .any(|cap| cap.eq_ignore_ascii_case("host-bus"))
        );
        assert!(matches!(details.kind, SessionKind::Host));
    }

    #[test]
    fn handshake_log_message_includes_core_fields() {
        let outcome = HandshakeSessionOutcome::granted();
        let addr: std::net::SocketAddr = "127.0.0.1:12345".parse().unwrap();
        let message = handshake_log_message(
            1_700_000_000,
            "devbox",
            Some(addr),
            &vec!["metrics".to_string(), "bus-v1".to_string()],
            SessionKind::Guest,
            &outcome,
        );
        assert!(message.contains("handshake ts=1700000000"));
        assert!(message.contains("vm=devbox"));
        assert!(message.contains("remote=127.0.0.1:12345"));
        assert!(message.contains("capabilities=[metrics,bus-v1]"));
        assert!(message.contains("session_kind=guest"));
        assert!(message.contains("session_outcome=granted"));
    }

    #[test]
    fn append_handshake_event_persists_json_record() {
        let dir = tempdir().unwrap();
        let outcome = HandshakeSessionOutcome::denied("missing-capability");
        let record = BrokerHandshakeEventRecord::new(
            1_700_000_001,
            "alpha",
            &vec!["bus-v1".to_string(), "BUS-V1".to_string()],
            SessionKind::Guest,
            &outcome,
            Some("127.0.0.1:2222"),
        );
        append_handshake_event(dir.path(), &record).expect("handshake event");
        let path = dir.path().join(HANDSHAKE_EVENT_LOG);
        let raw = fs::read_to_string(path).unwrap();
        let json: serde_json::Value = serde_json::from_str(raw.trim()).unwrap();
        assert_eq!(json["vm"], "alpha");
        assert_eq!(json["timestamp"], 1_700_000_001);
        assert_eq!(json["session_outcome"], "denied");
        assert_eq!(json["reason"], "missing-capability");
        assert_eq!(json["session_kind"], "guest");
        assert_eq!(json["capabilities"], serde_json::json!(["bus-v1"]));
        assert_eq!(json["remote_addr"], "127.0.0.1:2222");
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

    #[test]
    fn handle_publish_appends_targeted_vm_logs() {
        let dir = tempdir().unwrap();
        let frame = BusFrame {
            kind: "publish".to_string(),
            topic: Some("vm:alpha".to_string()),
            payload: json!({ "message": "hi" }),
        };
        handle_publish("host", &frame, dir.path()).expect("publish");
        let publisher_log = dir.path().join("host.log");
        assert!(publisher_log.exists());
        let publisher_contents = fs::read_to_string(&publisher_log).unwrap();
        assert!(publisher_contents.contains("\"topic\":\"vm:alpha\""));
        let target_log = dir.path().join("alpha.log");
        assert!(target_log.exists());
        let target_contents = fs::read_to_string(&target_log).unwrap();
        assert!(target_contents.contains("\"vm\":\"host\""));
        let shared = fs::read_to_string(dir.path().join("bus.log")).unwrap();
        assert!(shared.contains("\"topic\":\"vm:alpha\""));
    }
}
