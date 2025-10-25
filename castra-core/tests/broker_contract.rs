#![cfg(feature = "cli")]

use std::error::Error;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{self, Value, json};
use tempfile::TempDir;

const BROKER_READY_LINE: &str = "castra-broker 0.1 ready";
const BROKER_HOST: &str = "127.0.0.1";
const HANDSHAKE_EVENTS_FILE: &str = "handshake-events.jsonl";
const BUS_EVENTS_FILE: &str = "bus-events.jsonl";
const BUS_DIR_NAME: &str = "bus";
const BUS_MAX_FRAME_SIZE: usize = 64 * 1024;
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(2);

type TestResult<T = ()> = Result<T, Box<dyn Error>>;

#[derive(Debug, Deserialize, Clone)]
struct HandshakeEvent {
    timestamp: u64,
    vm: String,
    capabilities: Vec<String>,
    session_kind: String,
    session_outcome: String,
    #[serde(default)]
    reason: Option<String>,
    #[serde(default)]
    remote_addr: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
struct BusEvent {
    timestamp: u64,
    vm: String,
    session: String,
    reason: String,
    action: String,
    #[serde(default)]
    detail: Option<String>,
}

#[derive(Debug)]
struct BrokerHarness {
    _tempdir: TempDir,
    child: Child,
    port: u16,
    _pidfile: PathBuf,
    logfile: PathBuf,
    handshake_dir: PathBuf,
    bus_dir: PathBuf,
}

impl BrokerHarness {
    fn spawn() -> TestResult<Self> {
        let tempdir = TempDir::new()?;
        let pidfile = tempdir.path().join("broker.pid");
        let logfile = tempdir.path().join("broker.log");
        let handshake_dir = tempdir.path().join("handshakes");
        let bus_dir = logfile
            .parent()
            .unwrap_or_else(|| tempdir.path())
            .join(BUS_DIR_NAME);

        let listener = TcpListener::bind((BROKER_HOST, 0))?;
        let port = listener.local_addr()?.port();
        drop(listener);

        let mut command = Command::new(env!("CARGO_BIN_EXE_castra"));
        command
            .arg("broker")
            .arg("--port")
            .arg(port.to_string())
            .arg("--pidfile")
            .arg(&pidfile)
            .arg("--logfile")
            .arg(&logfile)
            .arg("--handshake-dir")
            .arg(&handshake_dir)
            .stdout(Stdio::null())
            .stderr(Stdio::piped());

        let child = command.spawn().map_err(|err| {
            io::Error::new(
                io::ErrorKind::Other,
                format!(
                    "failed to spawn broker binary {}: {err}",
                    env!("CARGO_BIN_EXE_castra")
                ),
            )
        })?;

        wait_for_broker_ready(port)?;

        Ok(Self {
            _tempdir: tempdir,
            child,
            port,
            _pidfile: pidfile,
            logfile,
            handshake_dir,
            bus_dir,
        })
    }

    fn connect(&self) -> io::Result<BrokerClient> {
        let mut stream = TcpStream::connect((BROKER_HOST, self.port))?;
        stream.set_read_timeout(Some(Duration::from_secs(5)))?;
        stream.set_write_timeout(Some(Duration::from_secs(5)))?;
        let greeting = read_line(&mut stream)?;
        if greeting.trim() != BROKER_READY_LINE {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("unexpected broker greeting: {greeting}"),
            ));
        }
        Ok(BrokerClient { stream })
    }

    fn handshake_events(&self) -> io::Result<Vec<HandshakeEvent>> {
        let path = self.handshake_dir.join(HANDSHAKE_EVENTS_FILE);
        read_json_lines(&path)
    }

    fn bus_events(&self) -> io::Result<Vec<BusEvent>> {
        let path = self.handshake_dir.join(BUS_EVENTS_FILE);
        read_json_lines(&path)
    }

    fn bus_log_entries(&self, vm: &str) -> io::Result<Vec<Value>> {
        let path = self.bus_dir.join(format!("{vm}.log"));
        read_json_lines(&path)
    }

    fn broker_log(&self) -> io::Result<String> {
        std::fs::read_to_string(&self.logfile)
    }
}

impl Drop for BrokerHarness {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[derive(Debug)]
struct BrokerClient {
    stream: TcpStream,
}

#[derive(Debug, Clone)]
struct HandshakeAck {
    session: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AckFrame {
    #[serde(rename = "type")]
    frame_type: String,
    ack: String,
    status: String,
    #[serde(default)]
    topic: Option<String>,
    #[serde(default)]
    reason: Option<String>,
}

impl BrokerClient {
    fn send_handshake(
        &mut self,
        identity: &str,
        capabilities: &[&str],
    ) -> io::Result<HandshakeAck> {
        let mut payload = format!("hello vm:{identity}");
        if !capabilities.is_empty() {
            payload.push(' ');
            payload.push_str("capabilities=");
            payload.push_str(&capabilities.join(","));
        }
        payload.push('\n');
        self.stream.write_all(payload.as_bytes())?;
        self.stream.flush()?;

        let response = read_line(&mut self.stream)?;
        if !response.starts_with("ok") {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                format!("broker rejected handshake: {response}"),
            ));
        }

        let session = response
            .split_whitespace()
            .find_map(|token| token.strip_prefix("session="))
            .map(|value| value.to_string());

        Ok(HandshakeAck { session })
    }

    fn local_addr(&self) -> io::Result<SocketAddr> {
        self.stream.local_addr()
    }

    fn send_subscribe(&mut self, topic: &str) -> io::Result<()> {
        let frame = json!({
            "type": "subscribe",
            "topic": topic,
        });
        self.send_frame(&frame)
    }

    fn send_publish(&mut self, topic: Option<&str>, payload: &Value) -> io::Result<()> {
        let mut frame = serde_json::Map::new();
        frame.insert("type".to_string(), Value::String("publish".to_string()));
        if let Some(topic) = topic {
            frame.insert("topic".to_string(), Value::String(topic.to_string()));
        }
        frame.insert("payload".to_string(), payload.clone());
        self.send_frame(&Value::Object(frame))
    }

    fn read_ack(&mut self) -> io::Result<(usize, AckFrame)> {
        let (len, value) = self.read_frame()?;
        let ack: AckFrame = serde_json::from_value(value).map_err(|err| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("invalid ack frame: {err}"),
            )
        })?;
        Ok((len, ack))
    }

    fn send_frame(&mut self, value: &Value) -> io::Result<()> {
        let payload = serde_json::to_vec(value)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
        if payload.len() > BUS_MAX_FRAME_SIZE {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "frame length {} exceeds max {}",
                    payload.len(),
                    BUS_MAX_FRAME_SIZE
                ),
            ));
        }

        let len = (payload.len() as u32).to_be_bytes();
        self.stream.write_all(&len)?;
        self.stream.write_all(&payload)?;
        self.stream.flush()?;
        Ok(())
    }

    fn read_frame(&mut self) -> io::Result<(usize, Value)> {
        let mut len_buf = [0u8; 4];
        self.stream.read_exact(&mut len_buf)?;
        let len = u32::from_be_bytes(len_buf) as usize;
        if len == 0 || len > BUS_MAX_FRAME_SIZE {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("invalid frame length {len}"),
            ));
        }

        let mut payload = vec![0u8; len];
        self.stream.read_exact(&mut payload)?;
        let value = serde_json::from_slice(&payload)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
        Ok((len, value))
    }
}

fn wait_for_broker_ready(port: u16) -> io::Result<()> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match TcpStream::connect((BROKER_HOST, port)) {
            Ok(mut stream) => {
                stream.set_read_timeout(Some(Duration::from_secs(5)))?;
                stream.set_write_timeout(Some(Duration::from_secs(5)))?;
                match read_line(&mut stream) {
                    Ok(line) if line.trim() == BROKER_READY_LINE => {
                        let _ = stream.write_all(b"hello vm:ready-check capabilities=metrics\n");
                        let _ = stream.flush();
                        let _ = read_line(&mut stream);
                        return Ok(());
                    }
                    Ok(_) => continue,
                    Err(err) if err.kind() == io::ErrorKind::WouldBlock => continue,
                    Err(err) if err.kind() == io::ErrorKind::TimedOut => continue,
                    Err(_) => continue,
                }
            }
            Err(err) => {
                if Instant::now() >= deadline {
                    return Err(err);
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    }
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
                        "connection closed before line read",
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
            Err(err) if err.kind() == io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(10));
                continue;
            }
            Err(err) => return Err(err),
        }
        if buffer.len() >= 512 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "line exceeded 512 bytes",
            ));
        }
    }
    Ok(String::from_utf8_lossy(&buffer).trim().to_string())
}

fn read_json_lines<T>(path: &Path) -> io::Result<Vec<T>>
where
    T: DeserializeOwned,
{
    let file = match File::open(path) {
        Ok(file) => file,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => return Err(err),
    };
    let reader = BufReader::new(file);
    let mut entries = Vec::new();
    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let value = serde_json::from_str(&line)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
        entries.push(value);
    }
    Ok(entries)
}

fn wait_for_match<T, F>(timeout: Duration, mut poll: F) -> Option<T>
where
    F: FnMut() -> Option<T>,
    T: Clone,
{
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(value) = poll() {
            return Some(value);
        }
        if Instant::now() >= deadline {
            return None;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn broker_happy_path_guest_handshake() -> TestResult {
    let harness = BrokerHarness::spawn()?;
    let mut client = harness.connect()?;
    let ack = client.send_handshake("alpha", &["bus-v1"])?;
    let session = ack.session.clone().expect("session token issued");
    assert!(
        session.starts_with("sess-"),
        "invalid session token: {session}"
    );
    drop(client);

    let event = wait_for_match(DEFAULT_TIMEOUT, || {
        harness
            .handshake_events()
            .ok()
            .and_then(|events| events.into_iter().find(|event| event.vm == "alpha"))
    })
    .expect("handshake event for alpha");

    assert_eq!(event.session_outcome, "granted");
    assert_eq!(event.session_kind, "guest");
    assert!(event.reason.is_none());
    assert!(event.timestamp > 0);
    assert!(
        event.capabilities.iter().any(|cap| cap == "bus-v1"),
        "expected bus-v1 capability in recorded event: {event:?}"
    );
    assert!(
        event.remote_addr.is_some(),
        "expected remote address to be recorded"
    );

    Ok(())
}

#[test]
fn broker_denies_missing_capability() -> TestResult {
    let harness = BrokerHarness::spawn()?;
    let mut client = harness.connect()?;
    let ack = client.send_handshake("beta", &["metrics"])?;
    assert!(
        ack.session.is_none(),
        "missing capability handshake should not yield session"
    );
    drop(client);

    let event = wait_for_match(DEFAULT_TIMEOUT, || {
        harness
            .handshake_events()
            .ok()
            .and_then(|events| events.into_iter().find(|event| event.vm == "beta"))
    })
    .expect("handshake event for beta");

    assert_eq!(event.session_outcome, "denied");
    assert_eq!(event.reason.as_deref(), Some("missing-capability"));
    assert!(
        event.capabilities.iter().any(|cap| cap == "metrics"),
        "expected metrics capability in recorded event: {event:?}"
    );
    assert!(event.remote_addr.is_some(), "expected remote address");

    Ok(())
}

#[test]
fn broker_guards_reserved_identity() -> TestResult {
    let harness = BrokerHarness::spawn()?;
    let mut client = harness.connect()?;
    let ack = client.send_handshake("host", &["bus-v1"])?;
    assert!(
        ack.session.is_none(),
        "reserved identity handshake should not grant session"
    );
    drop(client);

    let event = wait_for_match(DEFAULT_TIMEOUT, || {
        harness.handshake_events().ok().and_then(|events| {
            events
                .into_iter()
                .find(|event| event.vm.eq_ignore_ascii_case("host"))
        })
    })
    .expect("handshake event for host");

    assert_eq!(event.session_outcome, "denied");
    assert_eq!(event.reason.as_deref(), Some("reserved-identity"));
    assert!(
        event.capabilities.iter().any(|cap| cap == "bus-v1"),
        "expected bus-v1 capability in recorded event: {event:?}"
    );
    assert!(event.remote_addr.is_some(), "expected remote address");

    Ok(())
}

#[test]
fn broker_records_handshake_timeout() -> TestResult {
    let harness = BrokerHarness::spawn()?;
    let client = harness.connect()?;
    let remote = client.local_addr()?;
    // Hold the socket beyond the broker's read timeout without sending a payload.
    std::thread::sleep(Duration::from_secs(6));
    drop(client);

    let expected_vm = remote.to_string();
    let event = wait_for_match(Duration::from_secs(2), || {
        harness.handshake_events().ok().and_then(|events| {
            events
                .into_iter()
                .find(|event| event.vm == expected_vm && event.session_outcome == "timeout")
        })
    })
    .expect("handshake timeout event");

    assert_eq!(event.reason.as_deref(), Some("read-timeout"));
    assert!(
        event.capabilities.is_empty(),
        "timeout should not record capabilities"
    );
    assert!(event.remote_addr.is_some(), "expected remote address");

    Ok(())
}

#[test]
fn broker_bus_session_round_trip() -> TestResult {
    let harness = BrokerHarness::spawn()?;
    let mut client = harness.connect()?;
    let ack = client.send_handshake("gamma", &["bus-v1", "metrics"])?;
    let session = ack.session.clone().expect("bus session token");
    assert!(
        session.starts_with("sess-"),
        "invalid session token: {session}"
    );

    client.send_subscribe("updates")?;
    let (len, ack_frame) = client.read_ack()?;
    assert!(len <= BUS_MAX_FRAME_SIZE);
    assert_eq!(ack_frame.frame_type, "ack");
    assert_eq!(ack_frame.ack, "subscribe");
    assert_eq!(ack_frame.status, "ok");
    assert_eq!(ack_frame.topic.as_deref(), Some("updates"));
    assert!(ack_frame.reason.is_none());

    let payload = json!({"message": "hello bus"});
    client.send_publish(Some("updates"), &payload)?;
    let (len, publish_ack) = client.read_ack()?;
    assert!(len <= BUS_MAX_FRAME_SIZE);
    assert_eq!(publish_ack.frame_type, "ack");
    assert_eq!(publish_ack.ack, "publish");
    assert_eq!(publish_ack.status, "ok");
    assert_eq!(publish_ack.topic.as_deref(), Some("updates"));
    assert!(publish_ack.reason.is_none());

    drop(client);

    let bus_event = wait_for_match(DEFAULT_TIMEOUT, || {
        harness.bus_events().ok().and_then(|events| {
            events.into_iter().find(|event| {
                event.vm == "gamma" && event.session == session && event.action == "disconnect"
            })
        })
    })
    .expect("bus event for gamma");

    assert_eq!(bus_event.reason, "io_error");
    assert!(bus_event.timestamp > 0);
    assert!(
        bus_event
            .detail
            .as_deref()
            .map(|detail| detail.contains("io_kind"))
            .unwrap_or(false),
        "expected io_kind detail in bus event: {bus_event:?}"
    );

    let logs = harness.bus_log_entries("gamma")?;
    let shared = harness.bus_log_entries("bus")?;
    assert!(
        !logs.is_empty(),
        "expected VM-specific bus log entries: {:?}",
        harness.broker_log().ok()
    );
    assert!(
        !shared.is_empty(),
        "expected shared bus log entries: {:?}",
        harness.broker_log().ok()
    );
    let last_entry = logs.last().cloned().expect("publish entry present");
    let payload_value = last_entry.get("payload").expect("payload in bus log");
    assert_eq!(payload_value, &payload);

    Ok(())
}
