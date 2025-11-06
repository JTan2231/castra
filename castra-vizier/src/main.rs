use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use clap::Parser;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use time::OffsetDateTime;

const DEFAULT_PROTOCOL_VERSION: &str = "1.0.0";
const DEFAULT_VIZIER_VERSION: &str = env!("CARGO_PKG_VERSION");
const DEFAULT_ECHO_LATENCY_MS: u64 = 150;

#[derive(Debug, Error)]
enum VizierError {
    #[error("i/o error: {0}")]
    Io(#[from] io::Error),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("stdout write error: {0}")]
    Stdout(io::Error),

    #[error("stdin read error: {0}")]
    Stdin(io::Error),
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Path to the Vizier runtime configuration JSON.
    #[arg(long)]
    config: Option<PathBuf>,

    /// Override the VM identifier advertised in the handshake.
    #[arg(long)]
    vm: Option<String>,

    /// Override the log directory used by the Vizier service.
    #[arg(long)]
    log_dir: Option<PathBuf>,

    /// Emit a single handshake frame and exit.
    #[arg(long)]
    probe: bool,
}

#[derive(Debug, Deserialize)]
#[serde(default)]
struct VizierConfig {
    vm: Option<String>,
    protocol_version: String,
    vm_vizier_version: String,
    log_dir: PathBuf,
    #[serde(default)]
    capabilities: CapabilityConfig,
}

impl Default for VizierConfig {
    fn default() -> Self {
        Self {
            vm: None,
            protocol_version: DEFAULT_PROTOCOL_VERSION.to_string(),
            vm_vizier_version: DEFAULT_VIZIER_VERSION.to_string(),
            log_dir: PathBuf::from("/var/log/castra/vizier"),
            capabilities: CapabilityConfig::default(),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(default)]
struct CapabilityConfig {
    echo_latency_hint_ms: Option<u64>,
    supports_reconnect: Option<bool>,
    supports_system_events: Option<bool>,
}

impl Default for CapabilityConfig {
    fn default() -> Self {
        Self {
            echo_latency_hint_ms: Some(DEFAULT_ECHO_LATENCY_MS),
            supports_reconnect: Some(true),
            supports_system_events: Some(true),
        }
    }
}

impl VizierConfig {
    fn load(path: &Path) -> Result<Self, VizierError> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        let config: VizierConfig = serde_json::from_reader(reader)?;
        Ok(config)
    }

    fn apply_overrides(mut self, cli: &Cli) -> Self {
        if let Some(vm) = cli.vm.as_ref() {
            self.vm = Some(vm.clone());
        }
        if let Some(dir) = cli.log_dir.as_ref() {
            self.log_dir = dir.clone();
        }
        self
    }
}

#[derive(Serialize)]
struct HandshakeFrame<'a> {
    #[serde(rename = "type")]
    frame_type: &'a str,
    protocol_version: &'a str,
    vm_vizier_version: &'a str,
    vm: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    capabilities: Option<Capabilities<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    log_path: Option<&'a str>,
}

#[derive(Serialize)]
struct Capabilities<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    echo_latency_hint_ms: Option<&'a u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    supports_reconnect: Option<&'a bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    supports_system_events: Option<&'a bool>,
}

#[derive(Serialize)]
struct AckFrame<'a> {
    #[serde(rename = "type")]
    frame_type: &'a str,
    id: &'a str,
    received_at_ms: u128,
}

#[derive(Deserialize)]
struct InputFrame {
    #[serde(rename = "type")]
    frame_type: String,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    message: Option<String>,
}

#[derive(Serialize)]
struct OutputFrame<'a> {
    #[serde(rename = "type")]
    frame_type: &'a str,
    stream: &'a str,
    message: &'a str,
}

#[derive(Serialize)]
struct ErrorFrame<'a> {
    #[serde(rename = "type")]
    frame_type: &'a str,
    scope: &'a str,
    message: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    raw: Option<&'a str>,
}

fn main() -> Result<(), VizierError> {
    let cli = Cli::parse();
    let config = if let Some(path) = cli.config.as_ref() {
        VizierConfig::load(path)?.apply_overrides(&cli)
    } else {
        VizierConfig::default().apply_overrides(&cli)
    };

    let vm = config.vm.as_deref().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "vm must be provided via config or --vm flag",
        )
    })?;

    let log_dir = config.log_dir.join(vm);
    fs::create_dir_all(&log_dir)?;
    let mut logger = Logger::new(&log_dir.join("service.log"))?;

    let log_path_string = log_dir.join("service.log").display().to_string();
    let capabilities = Capabilities {
        echo_latency_hint_ms: config.capabilities.echo_latency_hint_ms.as_ref(),
        supports_reconnect: config.capabilities.supports_reconnect.as_ref(),
        supports_system_events: config.capabilities.supports_system_events.as_ref(),
    };

    let handshake = HandshakeFrame {
        frame_type: "handshake",
        protocol_version: config.protocol_version.as_str(),
        vm_vizier_version: config.vm_vizier_version.as_str(),
        vm,
        capabilities: Some(capabilities),
        log_path: Some(log_path_string.as_str()),
    };

    emit_frame(&handshake)?;
    logger.log("handshake emitted")?;

    if cli.probe {
        return Ok(());
    }

    run_service(&mut logger)?;
    Ok(())
}

fn run_service(logger: &mut Logger) -> Result<(), VizierError> {
    let stdin = io::stdin();
    let reader = stdin.lock();
    for line in reader.lines() {
        let line = line.map_err(VizierError::Stdin)?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        match serde_json::from_str::<InputFrame>(trimmed) {
            Ok(frame) if frame.frame_type == "input" => {
                if let Some(id) = frame.id.as_ref() {
                    let ack = AckFrame {
                        frame_type: "ack",
                        id,
                        received_at_ms: epoch_millis(),
                    };
                    emit_frame(&ack)?;
                }

                if let Some(message) = frame.message.as_ref() {
                    let output = OutputFrame {
                        frame_type: "output",
                        stream: "stdout",
                        message,
                    };
                    emit_frame(&output)?;
                    logger.log(&format!("input handled: {}", message))?;
                } else {
                    logger.log("input frame missing message")?;
                }
            }
            Ok(frame) => {
                let error = ErrorFrame {
                    frame_type: "error",
                    scope: "input",
                    message: "Unsupported frame type received.",
                    raw: Some(trimmed),
                };
                emit_frame(&error)?;
                logger.log(&format!("unsupported frame type: {}", frame.frame_type))?;
            }
            Err(err) => {
                let error = ErrorFrame {
                    frame_type: "error",
                    scope: "input",
                    message: "Failed to parse input frame.",
                    raw: Some(trimmed),
                };
                emit_frame(&error)?;
                logger.log(&format!("failed to parse input frame: {err}"))?;
            }
        }
    }
    Ok(())
}

fn emit_frame(frame: &impl Serialize) -> Result<(), VizierError> {
    let mut stdout = io::stdout().lock();
    let json = serde_json::to_string(frame)?;
    stdout
        .write_all(json.as_bytes())
        .and_then(|_| stdout.write_all(b"\n"))
        .and_then(|_| stdout.flush())
        .map_err(VizierError::Stdout)
}

fn epoch_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::from_secs(0))
        .as_millis()
}

struct Logger {
    file: File,
}

impl Logger {
    fn new(path: &Path) -> Result<Self, VizierError> {
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .write(true)
            .open(path)?;
        Ok(Self { file })
    }

    fn log(&mut self, message: &str) -> Result<(), VizierError> {
        let now = OffsetDateTime::now_utc();
        let timestamp = now
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_else(|_| "unknown".into());
        writeln!(self.file, "[{timestamp}] {message}").map_err(VizierError::Io)
    }
}
