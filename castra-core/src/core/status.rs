use std::collections::HashMap;
use std::fs;
use std::io::{self, ErrorKind};
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

    let (handshake_identities, mut identity_warnings) = handshake_identity_candidates(project);
    diagnostics.extend(identity_warnings.drain(..));

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

        println!("{}: {}", vm.name, state);

        let record = handshake_identities
            .get(&vm.name)
            .and_then(|identities| {
                identities
                    .iter()
                    .find_map(|identity| handshakes.remove(identity))
            })
            .or_else(|| handshakes.remove(&vm.name));

        let (bus_subscribed, last_publish_age, last_heartbeat_age) =
            bus_state_for_vm(record.as_ref(), now, vm.name.as_str(), &mut diagnostics);

        let (reachability, handshake_age, timestamp) = broker_reachability_for_vm(
            &broker_state,
            record.as_ref(),
            now,
            vm.name.as_str(),
            last_publish_age,
            last_heartbeat_age,
            &mut diagnostics,
        );

        if matches!(reachability, BrokerReachability::Reachable) {
            reachable = true;
        }

        if state != "running" {
            cleanup_orphan_overlay(&vm.name, &vm.overlay, &mut diagnostics);
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
            bus_subscribed,
            last_publish_age,
            last_heartbeat_age,
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
            .with_help("Confirm guest agents use the configured VM name, declare `handshake_identity` in bootstrap metadata, or prune stale handshake files."),
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

fn handshake_identity_candidates(
    project: &ProjectConfig,
) -> (HashMap<String, Vec<String>>, Vec<Diagnostic>) {
    let mut map = HashMap::new();
    let mut diagnostics = Vec::new();

    for vm in &project.vms {
        let mut identities = Vec::with_capacity(2);
        identities.push(vm.name.clone());

        if let Some(script_path) = vm.bootstrap.script.as_ref() {
            if let Some(dir) = script_path.parent() {
                let metadata_path = dir.join("bootstrap.toml");
                if metadata_path.is_file() {
                    match read_handshake_identity(&metadata_path) {
                        Ok(Some(identity)) => {
                            if identity != vm.name && !identities.contains(&identity) {
                                identities.push(identity);
                            }
                        }
                        Ok(None) => {}
                        Err(message) => {
                            diagnostics.push(
                                Diagnostic::new(Severity::Warning, message).with_help(
                                    "Fix the bootstrap metadata so `castra status` can correlate handshake aliases.",
                                ),
                            );
                        }
                    }
                }
            }
        }

        map.insert(vm.name.clone(), identities);
    }

    (map, diagnostics)
}

fn read_handshake_identity(path: &Path) -> Result<Option<String>, String> {
    let contents = fs::read_to_string(path).map_err(|err| {
        format!(
            "Failed to read bootstrap metadata {}: {err}",
            path.display()
        )
    })?;

    #[derive(Default, Deserialize)]
    struct StatusBootstrapMetadata {
        #[serde(default)]
        handshake_identity: Option<String>,
    }

    let metadata: StatusBootstrapMetadata = toml::from_str(&contents).map_err(|err| {
        format!(
            "Failed to parse bootstrap metadata {}: {err}",
            path.display()
        )
    })?;

    if let Some(identity) = metadata.handshake_identity {
        let trimmed = identity.trim();
        if trimmed.is_empty() {
            return Err(format!(
                "bootstrap metadata {} declares an empty handshake_identity.",
                path.display()
            ));
        }
        return Ok(Some(trimmed.to_string()));
    }

    Ok(None)
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
    last_publish_age: Option<Duration>,
    last_heartbeat_age: Option<Duration>,
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
                    let handshake_fresh = age <= HANDSHAKE_FRESHNESS;
                    let bus_fresh = last_publish_age
                        .into_iter()
                        .chain(last_heartbeat_age.into_iter())
                        .any(|bus_age| bus_age <= HANDSHAKE_FRESHNESS);
                    let status = if handshake_fresh || bus_fresh {
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

fn bus_state_for_vm(
    record: Option<&HandshakeRecord>,
    now: SystemTime,
    vm_name: &str,
    diagnostics: &mut Vec<Diagnostic>,
) -> (bool, Option<Duration>, Option<Duration>) {
    let Some(record) = record else {
        return (false, None, None);
    };

    let Some(bus) = &record.bus else {
        return (false, None, None);
    };

    let (publish_age, publish_future) = duration_since_optional(now, bus.last_publish);
    if publish_future {
        diagnostics.push(
            Diagnostic::new(
                Severity::Warning,
                format!("Bus publish timestamp for VM `{vm_name}` is ahead of the host clock."),
            )
            .with_help("Ensure host and guest clocks are synchronized."),
        );
    }

    let (heartbeat_age, heartbeat_future) = duration_since_optional(now, bus.last_heartbeat);
    if heartbeat_future {
        diagnostics.push(
            Diagnostic::new(
                Severity::Warning,
                format!("Bus heartbeat timestamp for VM `{vm_name}` is ahead of the host clock."),
            )
            .with_help("Ensure host and guest clocks are synchronized."),
        );
    }

    (
        !bus.subscribed_topics.is_empty(),
        publish_age,
        heartbeat_age,
    )
}

fn duration_since_optional(
    now: SystemTime,
    timestamp: Option<SystemTime>,
) -> (Option<Duration>, bool) {
    match timestamp {
        Some(ts) => match now.duration_since(ts) {
            Ok(age) => (Some(age), false),
            Err(_) => (Some(Duration::from_secs(0)), true),
        },
        None => (None, false),
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

fn cleanup_orphan_overlay(vm_name: &str, overlay_path: &Path, diagnostics: &mut Vec<Diagnostic>) {
    match remove_overlay_if_present(overlay_path) {
        Ok(Some(bytes)) => {
            diagnostics.push(
                Diagnostic::new(
                    Severity::Info,
                    format!(
                        "Removed stale ephemeral overlay {} for VM `{vm_name}` (reclaimed {}).",
                        overlay_path.display(),
                        format_bytes(bytes)
                    ),
                )
                .with_help("Guest changes are discarded on shutdown. Export via SSH before stopping if you need to retain data."),
            );
        }
        Ok(None) => {}
        Err(err) => {
            diagnostics.push(
                Diagnostic::new(
                    Severity::Warning,
                    format!(
                        "Failed to remove stale ephemeral overlay {} for VM `{vm_name}`: {err}",
                        overlay_path.display()
                    ),
                )
                .with_help("Remove it manually (e.g. `rm <path>`) or run `castra clean --include-overlays`."),
            );
        }
    }
}

fn remove_overlay_if_present(path: &Path) -> io::Result<Option<u64>> {
    match fs::metadata(path) {
        Ok(metadata) => {
            if metadata.is_file() {
                let bytes = metadata.len();
                match fs::remove_file(path) {
                    Ok(_) => Ok(Some(bytes)),
                    Err(err) if err.kind() == ErrorKind::NotFound => Ok(None),
                    Err(err) => Err(err),
                }
            } else {
                Ok(None)
            }
        }
        Err(err) if err.kind() == ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err),
    }
}

fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} {}", UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
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

        let StoredHandshakeFile {
            vm,
            timestamp,
            mut capabilities,
            bus,
        } = stored;

        let vm = vm
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

        let timestamp =
            match convert_optional_timestamp("handshake", Some(timestamp), &path, &mut warnings) {
                Some(time) => time,
                None => continue,
            };

        capabilities.sort_unstable_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));
        capabilities.dedup_by(|a, b| a.eq_ignore_ascii_case(b));

        let bus_state = bus.map(|raw| {
            let StoredHandshakeBusFile {
                subscribed_topics,
                last_publish_ts,
                last_heartbeat_ts,
            } = raw;

            let mut topics = subscribed_topics;
            topics.sort_unstable_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));
            topics.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
            HandshakeBusState {
                subscribed_topics: topics,
                last_publish: convert_optional_timestamp(
                    "bus.last_publish",
                    last_publish_ts,
                    &path,
                    &mut warnings,
                ),
                last_heartbeat: convert_optional_timestamp(
                    "bus.last_heartbeat",
                    last_heartbeat_ts,
                    &path,
                    &mut warnings,
                ),
            }
        });

        let new_record = HandshakeRecord {
            timestamp,
            capabilities,
            bus: bus_state,
        };

        records
            .entry(vm)
            .and_modify(|existing: &mut HandshakeRecord| {
                if timestamp > existing.timestamp {
                    *existing = new_record.clone();
                } else if timestamp == existing.timestamp {
                    if existing.capabilities.is_empty() && !new_record.capabilities.is_empty() {
                        existing.capabilities = new_record.capabilities.clone();
                    }
                    if existing.bus.is_none() && new_record.bus.is_some() {
                        existing.bus = new_record.bus.clone();
                    }
                }
            })
            .or_insert(new_record);
    }

    (records, warnings)
}

fn fallback_identity(path: &Path) -> String {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .map(|value| value.to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

fn convert_optional_timestamp(
    label: &str,
    value: Option<u64>,
    path: &Path,
    warnings: &mut Vec<String>,
) -> Option<SystemTime> {
    value.and_then(
        |seconds| match UNIX_EPOCH.checked_add(Duration::from_secs(seconds)) {
            Some(time) => Some(time),
            None => {
                warnings.push(format!(
                    "Handshake file {} contains an out-of-range {label} timestamp {}.",
                    path.display(),
                    seconds
                ));
                None
            }
        },
    )
}

#[derive(Debug, Clone)]
struct HandshakeRecord {
    timestamp: SystemTime,
    capabilities: Vec<String>,
    bus: Option<HandshakeBusState>,
}

#[derive(Debug, Clone)]
struct HandshakeBusState {
    subscribed_topics: Vec<String>,
    last_publish: Option<SystemTime>,
    last_heartbeat: Option<SystemTime>,
}

#[derive(Debug, Deserialize)]
struct StoredHandshakeFile {
    vm: Option<String>,
    timestamp: u64,
    #[serde(default)]
    capabilities: Vec<String>,
    #[serde(default)]
    bus: Option<StoredHandshakeBusFile>,
}

#[derive(Debug, Deserialize)]
struct StoredHandshakeBusFile {
    #[serde(default)]
    subscribed_topics: Vec<String>,
    #[serde(default)]
    last_publish_ts: Option<u64>,
    #[serde(default)]
    last_heartbeat_ts: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        BaseImageSource, BootstrapConfig, BootstrapMode, BrokerConfig,
        DEFAULT_BOOTSTRAP_HANDSHAKE_WAIT_SECS, LifecycleConfig, MemorySpec, ProjectConfig,
        VmBootstrapConfig, VmDefinition, Workflows,
    };
    use std::collections::HashMap;
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
        assert!(!snapshot.rows[0].bus_subscribed);
        assert!(snapshot.rows[0].last_publish_age.is_none());
        assert!(snapshot.rows[0].last_heartbeat_age.is_none());
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
        assert!(!snapshot.rows[0].bus_subscribed);
        assert!(snapshot.rows[0].last_publish_age.is_none());
        assert!(snapshot.rows[0].last_heartbeat_age.is_none());
        assert!(snapshot.last_handshake.is_some());

        project.state_root = PathBuf::new();
    }

    #[test]
    fn collect_status_marks_reachable_with_fresh_bus_activity() {
        let dir = tempdir().unwrap();
        let mut project = sample_project(dir.path());
        write_broker_pid(dir.path());
        let stale_handshake = HANDSHAKE_FRESHNESS + Duration::from_secs(10);
        write_handshake_with_bus(
            dir.path(),
            "devbox",
            stale_handshake,
            true,
            None,
            Some(Duration::from_secs(5)),
        );

        let snapshot = collect_status(&project);

        assert!(snapshot.reachable);
        assert!(matches!(
            snapshot.rows[0].broker_reachability,
            BrokerReachability::Reachable
        ));
        let handshake_age = snapshot.rows[0]
            .handshake_age
            .expect("expected handshake age");
        assert!(
            handshake_age >= HANDSHAKE_FRESHNESS,
            "handshake age should remain stale to validate bus freshness handling"
        );
        let heartbeat_age = snapshot.rows[0]
            .last_heartbeat_age
            .expect("expected heartbeat age");
        assert!(
            heartbeat_age <= HANDSHAKE_FRESHNESS,
            "heartbeat should be treated as fresh"
        );

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
        assert!(!snapshot.rows[0].bus_subscribed);
        assert!(snapshot.rows[0].last_publish_age.is_none());
        assert!(snapshot.rows[0].last_heartbeat_age.is_none());

        project.state_root = PathBuf::new();
    }

    #[test]
    fn collect_status_cleans_stale_overlay_when_vm_stopped() {
        let dir = tempdir().unwrap();
        let mut project = sample_project(dir.path());
        let overlay_path = project.vms[0].overlay.clone();
        fs::write(&overlay_path, vec![0_u8; 256]).unwrap();

        let snapshot = collect_status(&project);

        assert!(
            snapshot
                .diagnostics
                .iter()
                .any(|diag| diag.message.contains("Removed stale ephemeral overlay")),
            "expected cleanup diagnostic, found: {:?}",
            snapshot.diagnostics
        );
        assert!(
            !overlay_path.exists(),
            "overlay {} should be removed",
            overlay_path.display()
        );

        project.state_root = PathBuf::new();
    }

    #[test]
    fn collect_status_handles_cleared_bus_session_state() {
        let dir = tempdir().unwrap();
        let mut project = sample_project(dir.path());
        write_broker_pid(dir.path());
        write_handshake_with_bus(
            dir.path(),
            "devbox",
            Duration::from_secs(2),
            false,
            None,
            None,
        );

        let snapshot = collect_status(&project);
        assert!(!snapshot.rows[0].bus_subscribed);
        assert!(snapshot.rows[0].last_publish_age.is_none());
        assert!(snapshot.rows[0].last_heartbeat_age.is_none());
        assert!(snapshot.rows[0].handshake_age.is_some());

        project.state_root = PathBuf::new();
    }

    #[test]
    fn collect_status_reports_bus_freshness() {
        let dir = tempdir().unwrap();
        let mut project = sample_project(dir.path());
        write_broker_pid(dir.path());
        write_handshake_with_bus(
            dir.path(),
            "devbox",
            Duration::from_secs(3),
            true,
            Some(Duration::from_secs(5)),
            Some(Duration::from_secs(2)),
        );

        let snapshot = collect_status(&project);
        assert!(snapshot.rows[0].bus_subscribed);
        let publish_age = snapshot.rows[0]
            .last_publish_age
            .expect("expected publish age");
        assert!(publish_age.as_secs() >= 4 && publish_age.as_secs() <= 6);
        let heartbeat_age = snapshot.rows[0]
            .last_heartbeat_age
            .expect("expected heartbeat age");
        assert!(heartbeat_age.as_secs() >= 1 && heartbeat_age.as_secs() <= 3);

        project.state_root = PathBuf::new();
    }

    #[test]
    fn collect_status_matches_handshake_alias_from_metadata() {
        let dir = tempdir().unwrap();
        let mut project = sample_project(dir.path());
        write_broker_pid(dir.path());

        let script_path = project.vms[0]
            .bootstrap
            .script
            .clone()
            .expect("script path");
        let script_dir = script_path
            .parent()
            .expect("bootstrap script should have a parent directory");
        fs::create_dir_all(script_dir).unwrap();
        let metadata_path = script_dir.join("bootstrap.toml");
        fs::write(&metadata_path, "handshake_identity = \"devbox-agent\"\n").unwrap();

        write_handshake(dir.path(), "devbox-agent", Duration::from_secs(2));

        let snapshot = collect_status(&project);
        assert!(snapshot.reachable);
        assert!(matches!(
            snapshot.rows[0].broker_reachability,
            BrokerReachability::Reachable
        ));
        assert!(
            !snapshot
                .diagnostics
                .iter()
                .any(|diag| diag.message.contains("no VM with that name")),
            "unexpected diagnostics: {:?}",
            snapshot.diagnostics
        );

        project.state_root = PathBuf::new();
    }

    fn sample_project(state_root: &std::path::Path) -> ProjectConfig {
        let project_root = state_root.to_path_buf();
        let bootstrap_dir = project_root.join("bootstrap").join("devbox");

        ProjectConfig {
            file_path: state_root.join("castra.toml"),
            project_root,
            version: "0.1.0".to_string(),
            project_name: "test".to_string(),
            vms: vec![VmDefinition {
                name: "devbox".to_string(),
                role_name: "devbox".to_string(),
                replica_index: 0,
                description: None,
                base_image: BaseImageSource::from_explicit(PathBuf::from("base.qcow2")),
                overlay: state_root.join("devbox-overlay.qcow2"),
                cpus: 2,
                memory: MemorySpec::new("2048 MiB", Some(2048 * 1024 * 1024)),
                port_forwards: Vec::new(),
                bootstrap: VmBootstrapConfig {
                    mode: BootstrapMode::Auto,
                    script: Some(bootstrap_dir.join("run.sh")),
                    payload: Some(bootstrap_dir.join("payload")),
                    handshake_timeout_secs: DEFAULT_BOOTSTRAP_HANDSHAKE_WAIT_SECS,
                    remote_dir: PathBuf::from("/tmp/castra-bootstrap"),
                    env: HashMap::new(),
                    verify: None,
                },
            }],
            state_root: state_root.to_path_buf(),
            workflows: Workflows { init: Vec::new() },
            broker: BrokerConfig { port: 7070 },
            lifecycle: LifecycleConfig::default(),
            bootstrap: BootstrapConfig::default(),
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

    fn write_handshake_with_bus(
        state_root: &std::path::Path,
        vm: &str,
        handshake_age: Duration,
        subscribed: bool,
        publish_age: Option<Duration>,
        heartbeat_age: Option<Duration>,
    ) {
        use serde_json::json;

        let dir = state_root.join("handshakes");
        fs::create_dir_all(&dir).unwrap();
        let target = dir.join(format!("{}.json", vm));

        let now = SystemTime::now();
        let handshake_ts = now.checked_sub(handshake_age).unwrap();
        let publish_ts = publish_age.and_then(|age| now.checked_sub(age));
        let heartbeat_ts = heartbeat_age.and_then(|age| now.checked_sub(age));

        let handshake_secs = handshake_ts.duration_since(UNIX_EPOCH).unwrap().as_secs();
        let publish_secs = publish_ts.map(|ts| ts.duration_since(UNIX_EPOCH).unwrap().as_secs());
        let heartbeat_secs =
            heartbeat_ts.map(|ts| ts.duration_since(UNIX_EPOCH).unwrap().as_secs());

        let subscribed_topics: Vec<&str> = if subscribed {
            vec!["broadcast"]
        } else {
            Vec::new()
        };

        let payload = json!({
            "vm": vm,
            "timestamp": handshake_secs,
            "capabilities": ["bus-v1"],
            "bus": {
                "protocol": "bus-v1",
                "subscribed_topics": subscribed_topics,
                "last_publish_ts": publish_secs,
                "last_heartbeat_ts": heartbeat_secs,
                "last_subscribe_ts": if subscribed { Some(handshake_secs) } else { None },
            }
        });

        fs::write(&target, serde_json::to_vec_pretty(&payload).unwrap()).unwrap();
    }
}
