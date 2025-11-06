use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{ChildStdin, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use async_channel::{Receiver, Sender, unbounded};
use castra_protocol::{ProtocolCompatibility, check_protocol_version, supported_protocol_range};
use log::{info, warn};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::HarnessError;

/// Configuration toggles controlling vizier remote tunnels.
#[derive(Clone, Debug)]
pub struct VizierRemoteConfig {
    pub handshake_timeout: Duration,
    pub max_reconnect_attempts: usize,
    pub max_backoff: Duration,
    pub ssh_program: String,
}

impl Default for VizierRemoteConfig {
    fn default() -> Self {
        Self {
            handshake_timeout: Duration::from_secs(2),
            max_reconnect_attempts: 5,
            max_backoff: Duration::from_secs(30),
            ssh_program: String::from("ssh"),
        }
    }
}

impl VizierRemoteConfig {
    pub fn with_handshake_timeout(mut self, timeout: Duration) -> Self {
        self.handshake_timeout = timeout;
        self
    }

    pub fn with_max_reconnect_attempts(mut self, attempts: usize) -> Self {
        self.max_reconnect_attempts = attempts;
        self
    }

    pub fn with_max_backoff(mut self, backoff: Duration) -> Self {
        self.max_backoff = backoff;
        self
    }

    pub fn with_ssh_program<S: Into<String>>(mut self, program: S) -> Self {
        self.ssh_program = program.into();
        self
    }
}

/// Plan describing how to attach to a vizier instance.
#[derive(Clone, Debug)]
pub struct VizierRemotePlan {
    pub vm: String,
    pub program: String,
    pub args: Vec<String>,
    pub log_path: Option<PathBuf>,
}

impl VizierRemotePlan {
    pub fn new<S: Into<String>>(vm: S, program: S, args: Vec<String>) -> Self {
        Self {
            vm: vm.into(),
            program: program.into(),
            args,
            log_path: None,
        }
    }

    pub fn with_log_path<P: Into<PathBuf>>(mut self, path: P) -> Self {
        self.log_path = Some(path.into());
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct VizierRemoteCapabilities {
    pub echo_latency_hint_ms: Option<u64>,
    pub supports_reconnect: Option<bool>,
    pub supports_system_events: Option<bool>,
}

/// Structured events streamed from vizier remote tunnels.
#[derive(Debug, Clone, PartialEq)]
pub enum VizierRemoteEvent {
    Handshake {
        vm: String,
        protocol_version: String,
        vm_vizier_version: String,
        log_path: Option<String>,
        capabilities: VizierRemoteCapabilities,
    },
    Output {
        vm: String,
        stream: String,
        message: String,
    },
    Status {
        vm: String,
        status: String,
        detail: Option<String>,
    },
    System {
        vm: String,
        category: String,
        message: String,
    },
    Usage {
        vm: String,
        prompt_tokens: i64,
        cached_tokens: i64,
        completion_tokens: i64,
    },
    Ack {
        vm: String,
        id: String,
    },
    Control {
        vm: String,
        event: String,
        reason: Option<String>,
    },
    Error {
        vm: String,
        scope: String,
        message: String,
        raw: Option<String>,
    },
    HandshakeFailed {
        vm: String,
        protocol_version: Option<String>,
        vm_vizier_version: Option<String>,
        message: String,
        remediation_hint: Option<String>,
    },
    ReconnectAttempt {
        vm: String,
        attempt: usize,
        wait_ms: u64,
    },
    ReconnectSucceeded {
        vm: String,
    },
    Disconnected {
        vm: String,
    },
}

#[derive(Debug, Clone)]
struct HandshakeSummary {
    protocol_version: String,
    vizier_version: String,
    log_path: Option<String>,
    capabilities: VizierRemoteCapabilities,
    latency: Duration,
    retries: usize,
}

#[derive(Debug, Clone)]
struct SessionOutcome {
    handshake: Option<HandshakeSummary>,
    frames: usize,
    duration: Duration,
}

/// Input frames sent to vizier.
#[derive(Debug, Clone, Serialize)]
pub struct VizierInputFrame {
    #[serde(rename = "type")]
    frame_type: String,
    id: String,
    message: String,
}

impl VizierInputFrame {
    pub fn new<S: Into<String>>(id: S, message: S) -> Self {
        Self {
            frame_type: "input".to_string(),
            id: id.into(),
            message: message.into(),
        }
    }
}

/// Maintains a single vizier remote tunnel, handling reconnects and frame parsing.
pub struct TunnelManager {
    events_rx: Receiver<VizierRemoteEvent>,
    input_tx: Sender<VizierInputFrame>,
    _worker: thread::JoinHandle<()>,
}

impl TunnelManager {
    pub fn new(plan: VizierRemotePlan, config: VizierRemoteConfig) -> Self {
        let (event_tx, event_rx) = unbounded();
        let (input_tx, input_rx) = unbounded();
        let worker = thread::spawn(move || {
            run_worker(plan, config, input_rx, event_tx);
        });

        Self {
            events_rx: event_rx,
            input_tx,
            _worker: worker,
        }
    }

    pub fn events(&self) -> Receiver<VizierRemoteEvent> {
        self.events_rx.clone()
    }

    pub fn send_input(&self, frame: VizierInputFrame) -> Result<(), HarnessError> {
        self.input_tx
            .try_send(frame)
            .map_err(|err| HarnessError::process_failure(None, err.to_string()))
    }
}

fn run_worker(
    plan: VizierRemotePlan,
    config: VizierRemoteConfig,
    input_rx: Receiver<VizierInputFrame>,
    event_tx: Sender<VizierRemoteEvent>,
) {
    let mut attempt = 0usize;
    let mut backoff = Duration::from_secs(1);

    loop {
        if attempt > 0 {
            if attempt >= config.max_reconnect_attempts {
                break;
            }
            let wait = backoff.min(config.max_backoff);
            let _ = event_tx.send_blocking(VizierRemoteEvent::ReconnectAttempt {
                vm: plan.vm.clone(),
                attempt,
                wait_ms: wait.as_millis() as u64,
            });
            thread::sleep(wait);
            backoff = wait * 2;
        }

        attempt += 1;

        let mut command = Command::new(&plan.program);
        command.args(&plan.args);
        command.stdin(Stdio::piped());
        command.stdout(Stdio::piped());
        command.stderr(Stdio::null());

        match command.spawn() {
            Ok(mut child) => {
                backoff = Duration::from_secs(1);
                let stdout = child.stdout.take();
                let stdin = child.stdin.take();

                if let (Some(stdout), Some(stdin)) = (stdout, stdin) {
                    let session_attempt = attempt;
                    let result = run_session(
                        &plan,
                        &config,
                        stdout,
                        stdin,
                        input_rx.clone(),
                        &event_tx,
                        session_attempt,
                    );
                    match result {
                        Ok(outcome) => {
                            if let Some(handshake) = outcome.handshake.as_ref() {
                                let summary = format_handshake_summary(handshake);
                                info!(
                                    "vizier tunnel closed: vm={} duration_ms={} frames={} {}",
                                    plan.vm,
                                    outcome.duration.as_millis(),
                                    outcome.frames,
                                    summary
                                );
                            } else {
                                info!(
                                    "vizier tunnel closed before handshake: vm={} duration_ms={} frames={}",
                                    plan.vm,
                                    outcome.duration.as_millis(),
                                    outcome.frames
                                );
                            }
                        }
                        Err(err) => {
                            warn!("vizier tunnel error (vm={}): {}", plan.vm, err);
                            let _ = event_tx.send_blocking(VizierRemoteEvent::Error {
                                vm: plan.vm.clone(),
                                scope: "session".to_string(),
                                message: err,
                                raw: None,
                            });
                        }
                    }
                }

                let _ = child.kill();
                let _ = child.wait();
                let _ = event_tx.send_blocking(VizierRemoteEvent::Disconnected {
                    vm: plan.vm.clone(),
                });
            }
            Err(err) => {
                let _ = event_tx.send_blocking(VizierRemoteEvent::Error {
                    vm: plan.vm.clone(),
                    scope: "spawn".to_string(),
                    message: format!("Failed to launch vizier tunnel: {err}"),
                    raw: None,
                });
            }
        }
    }
}

fn run_session(
    plan: &VizierRemotePlan,
    config: &VizierRemoteConfig,
    stdout: impl std::io::Read + Send + 'static,
    mut stdin: ChildStdin,
    input_rx: Receiver<VizierInputFrame>,
    event_tx: &Sender<VizierRemoteEvent>,
    attempt: usize,
) -> Result<SessionOutcome, String> {
    let vm = plan.vm.clone();
    let mut reader = BufReader::new(stdout);
    let mut line = String::new();
    let handshake_deadline = Instant::now() + config.handshake_timeout;
    let mut handshake_emitted = false;
    let session_start = Instant::now();
    let mut frames_seen = 0usize;
    let mut handshake_summary: Option<HandshakeSummary> = None;

    let writer_rx = input_rx.clone();
    let writer_handle = thread::spawn(move || {
        while let Ok(frame) = writer_rx.recv_blocking() {
            match serde_json::to_string(&frame) {
                Ok(payload) => {
                    if stdin.write_all(payload.as_bytes()).is_err() {
                        break;
                    }
                    if stdin.write_all(b"\n").is_err() {
                        break;
                    }
                    if stdin.flush().is_err() {
                        break;
                    }
                }
                Err(_) => {
                    break;
                }
            }
        }
    });

    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                match serde_json::from_str::<RawFrame>(trimmed) {
                    Ok(frame) => {
                        let frame_kind = frame.frame_type.clone();
                        if !handshake_emitted {
                            if frame_kind == "handshake" {
                                handshake_emitted = true;
                                match frame.into_event(vm.clone()) {
                                    Some(event) => {
                                        if let VizierRemoteEvent::Handshake {
                                            protocol_version,
                                            vm_vizier_version,
                                            log_path,
                                            capabilities,
                                            ..
                                        } = &event
                                        {
                                            let protocol_trimmed = protocol_version.trim();
                                            let vizier_version = vm_vizier_version.clone();
                                            let log_path_owned = log_path.clone();
                                            let capabilities_clone = capabilities.clone();
                                            if protocol_trimmed.is_empty() {
                                                let message =
                                                    "Vizier handshake missing protocol_version."
                                                        .to_string();
                                                warn!(
                                                    "vizier handshake rejected: vm={} vizier={} reason={} log_path={}",
                                                    vm,
                                                    vizier_version,
                                                    message,
                                                    log_path_owned.as_deref().unwrap_or("-")
                                                );
                                                send_handshake_failure(
                                                    &vm,
                                                    event_tx,
                                                    None,
                                                    Some(&vizier_version),
                                                    log_path_owned.as_deref(),
                                                    message.clone(),
                                                );
                                                return Err(message);
                                            }

                                            match check_protocol_version(protocol_trimmed) {
                                                Ok(ProtocolCompatibility::Supported) => {
                                                    let retries = attempt.saturating_sub(1);
                                                    let summary = HandshakeSummary {
                                                        protocol_version: protocol_version.clone(),
                                                        vizier_version: vizier_version.clone(),
                                                        log_path: log_path_owned.clone(),
                                                        capabilities: capabilities_clone,
                                                        latency: session_start.elapsed(),
                                                        retries,
                                                    };
                                                    let detail = format_handshake_summary(&summary);
                                                    info!(
                                                        "vizier tunnel connected: vm={} {}",
                                                        vm, detail
                                                    );
                                                    handshake_summary = Some(summary);
                                                    let _ = event_tx.send_blocking(event);
                                                    let _ = event_tx.send_blocking(
                                                        VizierRemoteEvent::ReconnectSucceeded {
                                                            vm: vm.clone(),
                                                        },
                                                    );
                                                    frames_seen += 1;
                                                }
                                                Ok(ProtocolCompatibility::BelowMinimum) => {
                                                    let message = format!(
                                                        "Vizier protocol {} is below supported {}.",
                                                        protocol_version,
                                                        supported_protocol_range()
                                                    );
                                                    warn!(
                                                        "vizier handshake rejected: vm={} protocol={} vizier={} reason={} log_path={}",
                                                        vm,
                                                        protocol_version,
                                                        vizier_version,
                                                        message,
                                                        log_path_owned.as_deref().unwrap_or("-")
                                                    );
                                                    send_handshake_failure(
                                                        &vm,
                                                        event_tx,
                                                        Some(protocol_version.as_str()),
                                                        Some(&vizier_version),
                                                        log_path_owned.as_deref(),
                                                        message.clone(),
                                                    );
                                                    return Err(message);
                                                }
                                                Ok(ProtocolCompatibility::AboveMaximum) => {
                                                    let message = format!(
                                                        "Vizier protocol {} exceeds supported {}.",
                                                        protocol_version,
                                                        supported_protocol_range()
                                                    );
                                                    warn!(
                                                        "vizier handshake rejected: vm={} protocol={} vizier={} reason={} log_path={}",
                                                        vm,
                                                        protocol_version,
                                                        vizier_version,
                                                        message,
                                                        log_path_owned.as_deref().unwrap_or("-")
                                                    );
                                                    send_handshake_failure(
                                                        &vm,
                                                        event_tx,
                                                        Some(protocol_version.as_str()),
                                                        Some(&vizier_version),
                                                        log_path_owned.as_deref(),
                                                        message.clone(),
                                                    );
                                                    return Err(message);
                                                }
                                                Err(err) => {
                                                    let message = format!(
                                                        "Vizier protocol `{}` is invalid: {}.",
                                                        protocol_version, err
                                                    );
                                                    warn!(
                                                        "vizier handshake rejected: vm={} protocol={} vizier={} reason={} log_path={}",
                                                        vm,
                                                        protocol_version,
                                                        vizier_version,
                                                        message,
                                                        log_path_owned.as_deref().unwrap_or("-")
                                                    );
                                                    send_handshake_failure(
                                                        &vm,
                                                        event_tx,
                                                        Some(protocol_version.as_str()),
                                                        Some(&vizier_version),
                                                        log_path_owned.as_deref(),
                                                        message.clone(),
                                                    );
                                                    return Err(message);
                                                }
                                            }
                                        } else {
                                            warn!(
                                                "expected handshake event but decoded {:?}",
                                                event
                                            );
                                            return Err(
                                                "Unexpected vizier handshake payload.".to_string()
                                            );
                                        }
                                    }
                                    None => {
                                        warn!("unable to decode vizier handshake frame");
                                        return Err(
                                            "Failed to decode vizier handshake frame.".to_string()
                                        );
                                    }
                                }
                            } else if Instant::now() > handshake_deadline {
                                return Err("Handshake timed out".to_string());
                            }
                        } else {
                            let event_opt = frame.into_event(vm.clone());
                            if let Some(event) = event_opt {
                                frames_seen += 1;
                                let _ = event_tx.send_blocking(event);
                            } else {
                                warn!("vizier frame `{}` dropped (unhandled type)", frame_kind);
                            }
                        }
                    }
                    Err(err) => {
                        return Err(format!("Failed to decode vizier frame: {err}"));
                    }
                }
            }
            Err(err) => return Err(format!("Error reading vizier stream: {err}")),
        }
    }

    let _ = writer_handle.join();
    let outcome = SessionOutcome {
        handshake: handshake_summary,
        frames: frames_seen,
        duration: session_start.elapsed(),
    };
    Ok(outcome)
}

#[derive(Debug, Deserialize)]
struct RawFrame {
    #[serde(rename = "type")]
    frame_type: String,
    #[serde(default)]
    stream: Option<String>,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    detail: Option<String>,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    prompt_tokens: Option<i64>,
    #[serde(default)]
    cached_tokens: Option<i64>,
    #[serde(default)]
    completion_tokens: Option<i64>,
    #[serde(default)]
    protocol_version: Option<String>,
    #[serde(default)]
    vm_vizier_version: Option<String>,
    #[serde(default)]
    vm: Option<String>,
    #[serde(default)]
    scope: Option<String>,
    #[serde(default)]
    log_path: Option<String>,
    #[serde(default)]
    capabilities: Option<Value>,
    #[serde(default)]
    event: Option<String>,
    #[serde(default)]
    reason: Option<String>,
    #[serde(default)]
    raw: Option<String>,
}

impl RawFrame {
    fn into_event(self, vm: String) -> Option<VizierRemoteEvent> {
        match self.frame_type.as_str() {
            "handshake" => {
                if let Some(frame_vm) = self.vm.as_ref() {
                    if frame_vm != &vm {
                        warn!(
                            "vizier handshake reported vm `{}` but tunnel is bound to `{}`",
                            frame_vm, vm
                        );
                    }
                }
                Some(VizierRemoteEvent::Handshake {
                    vm,
                    protocol_version: self.protocol_version.unwrap_or_default(),
                    vm_vizier_version: self.vm_vizier_version.unwrap_or_default(),
                    log_path: self.log_path,
                    capabilities: parse_capabilities(self.capabilities),
                })
            }
            "output" => Some(VizierRemoteEvent::Output {
                vm,
                stream: self.stream.unwrap_or_else(|| "stdout".to_string()),
                message: self.message.unwrap_or_default(),
            }),
            "status" => Some(VizierRemoteEvent::Status {
                vm,
                status: self.status.unwrap_or_default(),
                detail: self.detail,
            }),
            "system" => Some(VizierRemoteEvent::System {
                vm,
                category: self.stream.unwrap_or_else(|| "log".to_string()),
                message: self.message.unwrap_or_default(),
            }),
            "usage" => Some(VizierRemoteEvent::Usage {
                vm,
                prompt_tokens: self.prompt_tokens.unwrap_or_default(),
                cached_tokens: self.cached_tokens.unwrap_or_default(),
                completion_tokens: self.completion_tokens.unwrap_or_default(),
            }),
            "ack" => Some(VizierRemoteEvent::Ack {
                vm,
                id: self.id.unwrap_or_default(),
            }),
            "control" => {
                let Some(event) = self.event.filter(|value| !value.is_empty()) else {
                    warn!("vizier control frame missing event field");
                    return None;
                };
                Some(VizierRemoteEvent::Control {
                    vm,
                    event,
                    reason: self.reason,
                })
            }
            "error" => Some(VizierRemoteEvent::Error {
                vm,
                scope: self.scope.unwrap_or_else(|| "vizier".to_string()),
                message: self.message.unwrap_or_default(),
                raw: self.raw,
            }),
            _ => None,
        }
    }
}

fn parse_capabilities(value: Option<Value>) -> VizierRemoteCapabilities {
    let mut caps = VizierRemoteCapabilities::default();
    if let Some(Value::Object(map)) = value {
        if let Some(hint) = map.get("echo_latency_hint_ms").and_then(Value::as_u64) {
            caps.echo_latency_hint_ms = Some(hint);
        }
        if let Some(flag) = map.get("supports_reconnect").and_then(Value::as_bool) {
            caps.supports_reconnect = Some(flag);
        }
        if let Some(flag) = map.get("supports_system_events").and_then(Value::as_bool) {
            caps.supports_system_events = Some(flag);
        }
    }
    caps
}

fn format_handshake_summary(summary: &HandshakeSummary) -> String {
    let mut parts = vec![
        format!("protocol={}", summary.protocol_version),
        format!("vizier={}", summary.vizier_version),
        format!("retries={}", summary.retries),
        format!("handshake_ms={}", summary.latency.as_millis()),
    ];
    if let Some(ms) = summary.capabilities.echo_latency_hint_ms {
        parts.push(format!("echo_hint_ms={ms}"));
    }
    if summary.capabilities.supports_reconnect.unwrap_or(false) {
        parts.push("reconnect_capable=true".to_string());
    }
    if summary.capabilities.supports_system_events.unwrap_or(false) {
        parts.push("system_events=true".to_string());
    }
    if let Some(path) = summary.log_path.as_deref() {
        parts.push(format!("log_path={path}"));
    }
    parts.join(" ")
}

fn send_handshake_failure(
    vm: &str,
    event_tx: &Sender<VizierRemoteEvent>,
    protocol_version: Option<&str>,
    vizier_version: Option<&str>,
    log_path: Option<&str>,
    message: String,
) {
    let failure = VizierRemoteEvent::HandshakeFailed {
        vm: vm.to_string(),
        protocol_version: protocol_version.map(|value| value.to_string()),
        vm_vizier_version: vizier_version.map(|value| value.to_string()),
        message: message.clone(),
        remediation_hint: Some(handshake_remediation(log_path)),
    };
    let _ = event_tx.send_blocking(failure);
}

fn handshake_remediation(log_path: Option<&str>) -> String {
    match log_path {
        Some(path) if !path.is_empty() => format!(
            "Update vizier to a protocol within {}; inspect logs at {}.",
            supported_protocol_range(),
            path
        ),
        _ => format!(
            "Update vizier to a protocol within {}.",
            supported_protocol_range()
        ),
    }
}
