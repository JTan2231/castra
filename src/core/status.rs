use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::config::{PortForward, PortProtocol, ProjectConfig};

use super::diagnostics::{Diagnostic, Severity};
use super::outcome::{BrokerReachability, VmStatusRow};
use super::project::config_state_root;
use super::runtime::{
    BrokerProcessState, broker_handshake_dir_from_root, inspect_broker_state, inspect_vm_state,
};

use serde::Deserialize;

pub const HANDSHAKE_FRESHNESS: Duration = Duration::from_secs(45);

#[derive(Debug)]
pub struct StatusSnapshot {
    pub rows: Vec<VmStatusRow>,
    pub broker_state: BrokerProcessState,
    pub diagnostics: Vec<Diagnostic>,
    pub reachable: bool,
    pub last_handshake: Option<BrokerHandshake>,
}

#[derive(Debug, Clone)]
pub struct BrokerHandshake {
    pub vm: String,
    pub timestamp: SystemTime,
    pub age: Duration,
}

pub fn collect_status(project: &ProjectConfig) -> StatusSnapshot {
    let mut rows = Vec::with_capacity(project.vms.len());
    let mut diagnostics = Vec::new();
    let state_root = config_state_root(project);
    let broker_pidfile = broker_pid_path_from_root(&state_root);

    let (broker_state, mut broker_warnings) = inspect_broker_state(&broker_pidfile);
    diagnostics.extend(
        broker_warnings
            .drain(..)
            .map(|warning| Diagnostic::new(Severity::Warning, warning)),
    );

    let handshake_dir = broker_handshake_dir_from_root(&state_root);
    let (mut handshakes, mut handshake_warnings) = load_handshake_records(&handshake_dir);
    diagnostics.extend(handshake_warnings.drain(..).map(|warning| {
        Diagnostic::new(Severity::Warning, warning)
            .with_help("Handshake records are stored under the castra state root; clear corrupted files and allow guests to reconnect.")
    }));

    let mut reachable = false;
    let mut last_handshake: Option<BrokerHandshake> = None;
    let now = SystemTime::now();

    for vm in &project.vms {
        let pidfile = state_root.join(format!("{}.pid", vm.name));
        let (state, uptime, mut state_warnings) = inspect_vm_state(&pidfile, &vm.name);
        diagnostics.extend(
            state_warnings
                .drain(..)
                .map(|warning| Diagnostic::new(Severity::Warning, warning)),
        );

        let record = handshakes.remove(&vm.name);
        let (reachability, handshake_age, timestamp) = broker_reachability_for_vm(
            &broker_state,
            record.as_ref(),
            now,
            vm.name.as_str(),
            &mut diagnostics,
        );

        if matches!(reachability, BrokerReachability::Reachable) {
            reachable = true;
        }

        if let Some(ts) = timestamp {
            let age = handshake_age.unwrap_or_else(|| Duration::from_secs(0));
            if should_update_last_handshake(&last_handshake, ts) {
                last_handshake = Some(BrokerHandshake {
                    vm: vm.name.clone(),
                    timestamp: ts,
                    age,
                });
            }
        }

        rows.push(VmStatusRow {
            name: vm.name.clone(),
            state,
            cpus: vm.cpus,
            memory: vm.memory.original().replace(' ', ""),
            uptime,
            broker_reachability: reachability,
            handshake_age,
            forwards: format_port_forwards(&vm.port_forwards),
        });
    }

    for (vm, record) in handshakes {
        diagnostics.push(
            Diagnostic::new(
                Severity::Info,
                format!(
                    "Broker observed a handshake from `{vm}` at {:?} but no VM with that name is configured.",
                    record.timestamp
                ),
            )
            .with_help("Confirm guest agents use the configured VM name or prune stale handshake files."),
        );
    }

    StatusSnapshot {
        rows,
        broker_state,
        diagnostics,
        reachable,
        last_handshake,
    }
}

pub fn format_port_forwards(forwards: &[PortForward]) -> String {
    let mut parts = Vec::with_capacity(forwards.len());
    for forward in forwards {
        parts.push(format!(
            "{}->{}{}",
            forward.host,
            forward.guest,
            format_protocol(forward.protocol)
        ));
    }
    if parts.is_empty() {
        "—".to_string()
    } else {
        parts.join(", ")
    }
}

fn format_protocol(protocol: PortProtocol) -> String {
    match protocol {
        PortProtocol::Tcp => "/tcp".to_string(),
        PortProtocol::Udp => "/udp".to_string(),
    }
}

pub fn format_uptime(uptime: Option<Duration>) -> String {
    match uptime {
        Some(duration) => {
            let seconds = duration.as_secs();
            let hours = seconds / 3600;
            let minutes = (seconds % 3600) / 60;
            let seconds = seconds % 60;
            format!("{hours:02}:{minutes:02}:{seconds:02}")
        }
        None => "—".to_string(),
    }
}

fn broker_pid_path_from_root(state_root: &std::path::Path) -> std::path::PathBuf {
    state_root.join("broker.pid")
}

fn broker_reachability_for_vm(
    broker_state: &BrokerProcessState,
    record: Option<&HandshakeRecord>,
    now: SystemTime,
    vm_name: &str,
    diagnostics: &mut Vec<Diagnostic>,
) -> (BrokerReachability, Option<Duration>, Option<SystemTime>) {
    match broker_state {
        BrokerProcessState::Offline => {
            let handshake_age = record.and_then(|rec| now.duration_since(rec.timestamp).ok());
            (
                BrokerReachability::Offline,
                handshake_age,
                record.map(|rec| rec.timestamp),
            )
        }
        BrokerProcessState::Running { .. } => match record {
            Some(record) => match now.duration_since(record.timestamp) {
                Ok(age) => {
                    let status = if age <= HANDSHAKE_FRESHNESS {
                        BrokerReachability::Reachable
                    } else {
                        BrokerReachability::Waiting
                    };
                    (status, Some(age), Some(record.timestamp))
                }
                Err(_) => {
                    diagnostics.push(
                        Diagnostic::new(
                            Severity::Warning,
                            format!(
                                "Guest handshake timestamp for VM `{vm_name}` is ahead of host clock."
                            ),
                        )
                        .with_help("Ensure host and guest clocks are synchronized."),
                    );
                    (
                        BrokerReachability::Waiting,
                        Some(Duration::from_secs(0)),
                        Some(record.timestamp),
                    )
                }
            },
            None => (BrokerReachability::Waiting, None, None),
        },
    }
}

fn should_update_last_handshake(
    current: &Option<BrokerHandshake>,
    candidate_ts: SystemTime,
) -> bool {
    match current {
        Some(existing) => candidate_ts > existing.timestamp,
        None => true,
    }
}

fn load_handshake_records(dir: &Path) -> (HashMap<String, HandshakeRecord>, Vec<String>) {
    let mut records = HashMap::new();
    let mut warnings = Vec::new();

    if !dir.is_dir() {
        return (records, warnings);
    }

    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(err) => {
            warnings.push(format!(
                "Unable to enumerate handshake directory {}: {err}",
                dir.display()
            ));
            return (records, warnings);
        }
    };

    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(err) => {
                warnings.push(format!("Failed to read handshake entry: {err}"));
                continue;
            }
        };
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }

        let contents = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(err) => {
                warnings.push(format!(
                    "Unable to read handshake file {}: {err}",
                    path.display()
                ));
                continue;
            }
        };

        let stored: StoredHandshakeFile = match serde_json::from_slice(&contents) {
            Ok(value) => value,
            Err(err) => {
                warnings.push(format!(
                    "Ignoring malformed handshake file {}: {err}",
                    path.display()
                ));
                continue;
            }
        };

        let vm = stored
            .vm
            .unwrap_or_else(|| fallback_identity(&path))
            .trim()
            .to_string();
        if vm.is_empty() {
            warnings.push(format!(
                "Ignoring handshake file {} with empty identity.",
                path.display()
            ));
            continue;
        }

        let timestamp = match UNIX_EPOCH.checked_add(Duration::from_secs(stored.timestamp)) {
            Some(time) => time,
            None => {
                warnings.push(format!(
                    "Handshake file {} contains an out-of-range timestamp {}.",
                    path.display(),
                    stored.timestamp
                ));
                continue;
            }
        };

        records
            .entry(vm)
            .and_modify(|existing: &mut HandshakeRecord| {
                if timestamp > existing.timestamp {
                    existing.timestamp = timestamp;
                }
            })
            .or_insert(HandshakeRecord { timestamp });
    }

    (records, warnings)
}

fn fallback_identity(path: &Path) -> String {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .map(|value| value.to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

#[derive(Debug)]
struct HandshakeRecord {
    timestamp: SystemTime,
}

#[derive(Debug, Deserialize)]
struct StoredHandshakeFile {
    vm: Option<String>,
    timestamp: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        BaseImageSource, BrokerConfig, LifecycleConfig, MemorySpec, ProjectConfig, VmDefinition,
        Workflows,
    };
    use std::fs;
    use std::path::PathBuf;
    use tempfile::tempdir;

    #[test]
    fn collect_status_marks_reachable_with_fresh_handshake() {
        let dir = tempdir().unwrap();
        let mut project = sample_project(dir.path());
        write_broker_pid(dir.path());
        write_handshake(dir.path(), "devbox", Duration::from_secs(5));

        let snapshot = collect_status(&project);

        assert!(snapshot.diagnostics.is_empty());
        assert_eq!(snapshot.rows.len(), 1);
        assert!(snapshot.reachable);
        assert!(matches!(
            snapshot.rows[0].broker_reachability,
            BrokerReachability::Reachable
        ));
        assert!(snapshot.rows[0].handshake_age.is_some());
        let Some(handshake) = snapshot.last_handshake else {
            panic!("expected last handshake");
        };
        assert_eq!(handshake.vm, "devbox");

        // Avoid tempdir drop removing state root before project struct drop.
        project.state_root = PathBuf::new();
    }

    #[test]
    fn collect_status_marks_waiting_when_handshake_stale() {
        let dir = tempdir().unwrap();
        let mut project = sample_project(dir.path());
        write_broker_pid(dir.path());
        let stale = HANDSHAKE_FRESHNESS + Duration::from_secs(10);
        write_handshake(dir.path(), "devbox", stale);

        let snapshot = collect_status(&project);

        assert!(!snapshot.reachable);
        assert!(matches!(
            snapshot.rows[0].broker_reachability,
            BrokerReachability::Waiting
        ));
        assert!(
            snapshot.rows[0]
                .handshake_age
                .map(|age| age >= HANDSHAKE_FRESHNESS)
                .unwrap_or(false)
        );
        assert!(snapshot.last_handshake.is_some());

        project.state_root = PathBuf::new();
    }

    #[test]
    fn collect_status_handles_absent_handshake() {
        let dir = tempdir().unwrap();
        let mut project = sample_project(dir.path());
        write_broker_pid(dir.path());

        let snapshot = collect_status(&project);

        assert!(!snapshot.reachable);
        assert!(snapshot.last_handshake.is_none());
        assert!(matches!(
            snapshot.rows[0].broker_reachability,
            BrokerReachability::Waiting
        ));

        project.state_root = PathBuf::new();
    }

    fn sample_project(state_root: &std::path::Path) -> ProjectConfig {
        ProjectConfig {
            file_path: state_root.join("castra.toml"),
            version: "0.1.0".to_string(),
            project_name: "test".to_string(),
            vms: vec![VmDefinition {
                name: "devbox".to_string(),
                role_name: "devbox".to_string(),
                replica_index: 0,
                description: None,
                base_image: BaseImageSource::Path(PathBuf::from("base.qcow2")),
                overlay: state_root.join("devbox-overlay.qcow2"),
                cpus: 2,
                memory: MemorySpec::new("2048 MiB", Some(2048 * 1024 * 1024)),
                port_forwards: Vec::new(),
            }],
            state_root: state_root.to_path_buf(),
            workflows: Workflows { init: Vec::new() },
            broker: BrokerConfig { port: 7070 },
            lifecycle: LifecycleConfig::default(),
            warnings: Vec::new(),
        }
    }

    fn write_broker_pid(state_root: &std::path::Path) {
        let path = state_root.join("broker.pid");
        fs::write(&path, format!("{}\n", std::process::id())).unwrap();
    }

    fn write_handshake(state_root: &std::path::Path, vm: &str, age: Duration) {
        use serde_json::json;

        let dir = state_root.join("handshakes");
        fs::create_dir_all(&dir).unwrap();
        let target = dir.join(format!("{}.json", vm));
        let timestamp = SystemTime::now()
            .checked_sub(age)
            .unwrap()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let payload = json!({
            "vm": vm,
            "timestamp": timestamp,
        });
        fs::write(&target, serde_json::to_vec_pretty(&payload).unwrap()).unwrap();
    }
}
