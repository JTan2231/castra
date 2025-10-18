use std::cmp;
use std::collections::HashSet;
use std::fs;
use std::io::{self, ErrorKind, Read, Write};
#[cfg(unix)]
use std::io::{BufRead, BufReader};
use std::net::TcpListener;
#[cfg(unix)]
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::Sender;
use std::thread;
use std::time::{Duration, Instant, SystemTime};

use libc::{self, pid_t};
use sysinfo::{Disks, System};

use sha2::{Digest, Sha512};

use crate::config::{BaseImageProvenance, PortForward, PortProtocol, ProjectConfig, VmDefinition};
use crate::error::{Error, Result};
use serde_json::{Value, json};
use ureq::Error as UreqError;

use super::diagnostics::{Diagnostic, Severity};
use super::events::{
    CooperativeMethod, CooperativeTimeoutReason, EphemeralCleanupReason, Event, ShutdownOutcome,
    ShutdownSignal,
};

pub struct RuntimeContext {
    pub state_root: PathBuf,
    pub log_root: PathBuf,
    pub qemu_system: PathBuf,
    pub qemu_img: Option<PathBuf>,
    pub accelerators: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrokerProcessState {
    Running { pid: pid_t },
    Offline,
}

const DISK_WARN_THRESHOLD: u64 = 2 * 1024 * 1024 * 1024;
const DISK_FAIL_THRESHOLD: u64 = 500 * 1024 * 1024;
const MEMORY_WARN_HEADROOM: u64 = 1 * 1024 * 1024 * 1024;
const MEMORY_FAIL_HEADROOM: u64 = 512 * 1024 * 1024;
const DEFAULT_ALPINE_URL: &str =
    "https://github.com/JTan2231/castra/releases/download/alpine-x86_64.qcow2/alpine-x86_64.qcow2";
const DEFAULT_ALPINE_SHA512: &str = "10cd2d31e1d61c9dc323c4467cdc350c238e4234beb89bf6808681440180b477d51b3a16b7e522ebfd0c39dec3c22d593de87947a3f60d2ec67cde08685cc6c7";
const DEFAULT_ALPINE_SIZE_BYTES: u64 = 94_240_768;

#[derive(Debug)]
pub struct AssetPreparation {
    pub assets: ResolvedVmAssets,
    pub overlay_created: bool,
    pub overlay_reclaimed_bytes: Option<u64>,
    pub events: Vec<Event>,
}

#[derive(Debug)]
pub struct ResolvedVmAssets {
    pub boot: Option<BootOverrides>,
}

#[derive(Debug)]
pub struct BootOverrides {
    pub kernel: PathBuf,
    pub initrd: Option<PathBuf>,
    pub append: String,
    pub extra_args: Vec<String>,
    pub machine: Option<String>,
}

#[derive(Debug, Default)]
pub struct CheckOutcome {
    pub warnings: Vec<Diagnostic>,
    pub failures: Vec<String>,
}

pub fn prepare_runtime_context(project: &ProjectConfig) -> Result<RuntimeContext> {
    let state_root = super::project::config_state_root(project);
    fs::create_dir_all(&state_root).map_err(|err| Error::PreflightFailed {
        message: format!(
            "Failed to create castra state directory at {}: {err}",
            state_root.display()
        ),
    })?;

    let log_root = state_root.join("logs");
    fs::create_dir_all(&log_root).map_err(|err| Error::PreflightFailed {
        message: format!(
            "Failed to create log directory at {}: {err}",
            log_root.display()
        ),
    })?;

    let qemu_system = find_executable(&[
        "qemu-system-x86_64",
        "qemu-system-x86_64.exe",
        "qemu-system-aarch64",
    ])
    .ok_or_else(|| {
        Error::PreflightFailed {
            message: "qemu-system binary not found in PATH. Install QEMU (e.g. `brew install qemu` on macOS or `sudo apt install qemu-system` on Debian/Ubuntu).".to_string(),
        }
    })?;

    let qemu_img = find_executable(&["qemu-img", "qemu-img.exe"]);

    let image_storage_root = state_root.join("images");
    fs::create_dir_all(&image_storage_root).map_err(|err| Error::PreflightFailed {
        message: format!(
            "Failed to create image cache directory at {}: {err}",
            image_storage_root.display()
        ),
    })?;

    let accelerators = detect_available_accelerators(&qemu_system);

    Ok(RuntimeContext {
        state_root,
        log_root,
        qemu_system,
        qemu_img,
        accelerators,
    })
}

pub fn check_host_capacity(project: &ProjectConfig) -> CheckOutcome {
    let mut outcome = CheckOutcome::default();

    let requested_cpus: u32 = project.vms.iter().map(|vm| vm.cpus).sum();
    let system = System::new_all();
    let host_cpus = system.cpus().len() as u32;

    if requested_cpus > host_cpus {
        outcome.failures.push(format!(
            "Requested {} vCPUs but host has {} hardware threads.",
            requested_cpus, host_cpus
        ));
    } else if requested_cpus as f64 > (host_cpus as f64 * 0.8) {
        outcome.warnings.push(Diagnostic::new(
            Severity::Warning,
            format!(
                "VMs request {} vCPUs out of {host_cpus} available threads. Expect contention.",
                requested_cpus
            ),
        ));
    }

    let requested_memory: u64 = project.vms.iter().filter_map(|vm| vm.memory.bytes()).sum();

    if let Some(total_memory) = system.total_memory().checked_mul(1024) {
        let available = total_memory.saturating_sub(requested_memory);
        if available < MEMORY_FAIL_HEADROOM {
            outcome.failures.push(format!(
                "VMs request {} RAM; host free headroom would be {}.",
                format_bytes(requested_memory),
                format_bytes(available)
            ));
        } else if available < MEMORY_WARN_HEADROOM {
            outcome.warnings.push(Diagnostic::new(
                Severity::Warning,
                format!(
                    "VMs request {} RAM; host free headroom after launch estimated at {}.",
                    format_bytes(requested_memory),
                    format_bytes(available)
                ),
            ));
        }
    } else {
        outcome.warnings.push(Diagnostic::new(
            Severity::Warning,
            "Unable to determine host memory capacity; skipping memory safety check.",
        ));
    }

    outcome
}

pub fn check_disk_space(project: &ProjectConfig, context: &RuntimeContext) -> CheckOutcome {
    let mut paths: HashSet<PathBuf> = HashSet::new();
    paths.insert(context.state_root.clone());
    paths.insert(context.log_root.clone());
    for vm in &project.vms {
        if let Some(parent) = vm.overlay.parent() {
            paths.insert(parent.to_path_buf());
        } else {
            paths.insert(vm.overlay.clone());
        }
    }

    let mut disks = Disks::new_with_refreshed_list();
    let mut outcome = CheckOutcome::default();

    for path in paths {
        let probe = existing_directory(&path);
        let location = if probe == path {
            format!("{}", probe.display())
        } else {
            format!("{} (checked at {})", path.display(), probe.display())
        };

        match available_disk_space(&mut disks, &probe) {
            Some(space) if space < DISK_FAIL_THRESHOLD => {
                outcome.failures.push(format!(
                    "{location} has only {} free (requires at least {}).",
                    format_bytes(space),
                    format_bytes(DISK_FAIL_THRESHOLD),
                ));
            }
            Some(space) if space < DISK_WARN_THRESHOLD => {
                outcome.warnings.push(Diagnostic::new(
                    Severity::Warning,
                    format!(
                        "{location} has {} free; consider freeing space before launch (recommended {}).",
                        format_bytes(space),
                        format_bytes(DISK_WARN_THRESHOLD),
                    ),
                ));
            }
            Some(_) => {}
            None => outcome.warnings.push(Diagnostic::new(
                Severity::Warning,
                format!(
                    "Unable to determine free space at {location}; skipping disk safety check for this path."
                ),
            )),
        }
    }

    outcome
}

pub fn ensure_ports_available(project: &ProjectConfig) -> Result<()> {
    let (conflicts, broker_collision) = project.port_conflicts();
    if !conflicts.is_empty() {
        let mut lines = Vec::new();
        for conflict in conflicts {
            lines.push(format!(
                "- Port {} declared by: {}",
                conflict.port,
                conflict.vm_names.join(", ")
            ));
        }
        return Err(Error::PreflightFailed {
            message: format!("Host port conflicts detected:\n{}", lines.join("\n")),
        });
    }

    if let Some(collision) = broker_collision {
        return Err(Error::PreflightFailed {
            message: format!(
                "Host port {} is reserved for the castra broker. Update the broker port or VM forwards.",
                collision.port
            ),
        });
    }

    let mut checked = HashSet::new();
    for vm in &project.vms {
        for forward in &vm.port_forwards {
            if checked.insert(forward.host) {
                ensure_port_is_free(
                    forward.host,
                    &format!("forward `{}` on VM `{}`", forward.host, vm.name),
                )?;
            }
        }
    }

    if checked.insert(project.broker.port) {
        ensure_port_is_free(
            project.broker.port,
            &format!("broker port {}", project.broker.port),
        )?;
    }

    Ok(())
}

pub fn start_broker(
    project: &ProjectConfig,
    context: &RuntimeContext,
    diagnostics: &mut Vec<Diagnostic>,
    events: &mut Vec<Event>,
) -> Result<Option<u32>> {
    let pidfile = broker_pid_path(context);
    let (state, mut warnings) = inspect_broker_state(&pidfile);
    diagnostics.extend(
        warnings
            .drain(..)
            .map(|warning| Diagnostic::new(Severity::Warning, warning)),
    );

    if let BrokerProcessState::Running { pid } = state {
        events.push(Event::Message {
            severity: Severity::Info,
            text: format!(
                "→ broker: already running on 127.0.0.1:{} (pid {pid}).",
                project.broker.port
            ),
        });
        return Ok(None);
    }

    if pidfile.exists() {
        let _ = fs::remove_file(&pidfile);
    }

    let exe = std::env::current_exe().map_err(|err| Error::PreflightFailed {
        message: format!("Failed to determine current executable for broker launch: {err}"),
    })?;

    let log_path = broker_log_path(context);
    let handshake_dir = broker_handshake_dir(context);
    fs::create_dir_all(&handshake_dir).map_err(|err| Error::PreflightFailed {
        message: format!(
            "Failed to prepare broker handshake directory {}: {err}",
            handshake_dir.display()
        ),
    })?;

    let mut command = Command::new(exe);
    command
        .arg("broker")
        .arg("--port")
        .arg(project.broker.port.to_string())
        .arg("--pidfile")
        .arg(pidfile.as_os_str())
        .arg("--logfile")
        .arg(log_path.as_os_str())
        .arg("--handshake-dir")
        .arg(handshake_dir.as_os_str())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    let mut child = command.spawn().map_err(|err| Error::PreflightFailed {
        message: format!("Failed to launch broker subprocess: {err}"),
    })?;

    if let Err(err) = wait_for_pidfile(&pidfile, Duration::from_secs(3)) {
        let _ = child.kill();
        return Err(Error::PreflightFailed {
            message: format!("Broker process did not initialize: {err}"),
        });
    }

    let pid = child.id();
    events.push(Event::BrokerStarted {
        pid,
        port: project.broker.port,
    });

    Ok(Some(pid))
}

pub fn ensure_vm_assets(vm: &VmDefinition, context: &RuntimeContext) -> Result<AssetPreparation> {
    let mut events = Vec::new();
    let base_image_path = vm.base_image.path();

    match vm.base_image.provenance() {
        BaseImageProvenance::Explicit => {
            if !base_image_path.is_file() {
                return Err(Error::PreflightFailed {
                    message: format!(
                        "Base image for VM `{}` not found at {}. Update `base_image` or make sure the file exists.",
                        vm.name,
                        base_image_path.display()
                    ),
                });
            }
        }
        BaseImageProvenance::DefaultAlpine => {
            ensure_default_alpine_image(base_image_path, &mut events)?;
        }
    }

    let (overlay_created, overlay_reclaimed_bytes) = ensure_overlay(vm, context, base_image_path)?;

    Ok(AssetPreparation {
        assets: ResolvedVmAssets { boot: None },
        overlay_created,
        overlay_reclaimed_bytes,
        events,
    })
}

#[derive(Debug)]
enum AlpineCacheStatus {
    Valid,
    NeedsDownload { reason: String },
}

fn ensure_default_alpine_image(target: &Path, events: &mut Vec<Event>) -> Result<()> {
    let parent = target.parent().ok_or_else(|| Error::PreflightFailed {
        message: format!(
            "Unable to determine cache directory for default base image {}.",
            target.display()
        ),
    })?;
    fs::create_dir_all(parent).map_err(|err| Error::PreflightFailed {
        message: format!(
            "Failed to prepare image cache directory {}: {err}",
            parent.display()
        ),
    })?;

    match assess_alpine_cache(target)? {
        AlpineCacheStatus::Valid => return Ok(()),
        AlpineCacheStatus::NeedsDownload { reason } => {
            events.push(Event::Message {
                severity: Severity::Info,
                text: format!("{reason} Downloading a fresh copy from {DEFAULT_ALPINE_URL}."),
            });

            if let Err(err) = fs::remove_file(target) {
                if err.kind() != ErrorKind::NotFound {
                    return Err(Error::PreflightFailed {
                        message: format!(
                            "Failed to remove cached Alpine image at {}: {err}",
                            target.display()
                        ),
                    });
                }
            }

            let digest_path = cached_digest_path(target);
            if let Err(err) = fs::remove_file(&digest_path) {
                if err.kind() != ErrorKind::NotFound {
                    return Err(Error::PreflightFailed {
                        message: format!(
                            "Failed to remove cached digest {}: {err}",
                            digest_path.display()
                        ),
                    });
                }
            }
        }
    }

    events.push(Event::Message {
        severity: Severity::Info,
        text: format!("Downloading Alpine base image to {}...", target.display()),
    });

    let digest = download_default_alpine_image(target)?;
    write_cached_digest(&cached_digest_path(target), &digest)?;

    events.push(Event::Message {
        severity: Severity::Info,
        text: format!(
            "Cached Alpine base image at {} ({}).",
            target.display(),
            format_bytes(DEFAULT_ALPINE_SIZE_BYTES)
        ),
    });

    Ok(())
}

fn assess_alpine_cache(path: &Path) -> Result<AlpineCacheStatus> {
    if !path.is_file() {
        return Ok(AlpineCacheStatus::NeedsDownload {
            reason: format!("Default Alpine base image not found at {}.", path.display()),
        });
    }

    let metadata = fs::metadata(path).map_err(|err| Error::PreflightFailed {
        message: format!(
            "Failed to inspect cached Alpine base image at {}: {err}",
            path.display()
        ),
    })?;

    if metadata.len() != DEFAULT_ALPINE_SIZE_BYTES {
        return Ok(AlpineCacheStatus::NeedsDownload {
            reason: format!(
                "Cached Alpine base image at {} is {} bytes; expected {} bytes.",
                path.display(),
                metadata.len(),
                DEFAULT_ALPINE_SIZE_BYTES
            ),
        });
    }

    let digest_path = cached_digest_path(path);
    if let Some(stored) = read_cached_digest(&digest_path)? {
        if stored.eq_ignore_ascii_case(DEFAULT_ALPINE_SHA512) {
            return Ok(AlpineCacheStatus::Valid);
        }
    }

    let computed = compute_sha512_hex(path)?;
    if computed.eq_ignore_ascii_case(DEFAULT_ALPINE_SHA512) {
        write_cached_digest(&digest_path, &computed)?;
        return Ok(AlpineCacheStatus::Valid);
    }

    Ok(AlpineCacheStatus::NeedsDownload {
        reason: format!(
            "Cached Alpine base image at {} failed checksum verification.",
            path.display()
        ),
    })
}

fn cached_digest_path(path: &Path) -> PathBuf {
    match path.file_name().and_then(|name| name.to_str()) {
        Some(name) => path.with_file_name(format!("{name}.sha512")),
        None => path.with_extension("sha512"),
    }
}

fn read_cached_digest(path: &Path) -> Result<Option<String>> {
    match fs::read_to_string(path) {
        Ok(contents) => Ok(Some(contents.trim().to_string())),
        Err(err) if err.kind() == ErrorKind::NotFound => Ok(None),
        Err(err) => Err(Error::PreflightFailed {
            message: format!("Failed to read cached digest {}: {err}", path.display()),
        }),
    }
}

fn write_cached_digest(path: &Path, digest: &str) -> Result<()> {
    fs::write(path, format!("{digest}\n")).map_err(|err| Error::PreflightFailed {
        message: format!("Failed to persist digest file {}: {err}", path.display()),
    })
}

fn compute_sha512_hex(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path).map_err(|err| Error::PreflightFailed {
        message: format!(
            "Failed to open {} for checksum verification: {err}",
            path.display()
        ),
    })?;
    let mut hasher = Sha512::new();
    let mut buffer = [0u8; 64 * 1024];

    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|err| Error::PreflightFailed {
                message: format!(
                    "Failed while reading {} for checksum verification: {err}",
                    path.display()
                ),
            })?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }

    Ok(hex::encode(hasher.finalize()))
}

fn download_default_alpine_image(target: &Path) -> Result<String> {
    let temp_path = target.with_extension("download");
    if temp_path.exists() {
        let _ = fs::remove_file(&temp_path);
    }

    let response = ureq::get(DEFAULT_ALPINE_URL).call().map_err(|err| {
        let detail = match &err {
            UreqError::Status(code, _) => format!("server returned HTTP status {code}"),
            UreqError::Transport(inner) => inner.to_string(),
        };
        Error::PreflightFailed {
            message: format!(
                "Failed to download Alpine base image from {DEFAULT_ALPINE_URL}: {detail}. \
                 Check network connectivity or set an explicit `base_image` in castra.toml."
            ),
        }
    })?;

    let mut reader = response.into_reader();
    let mut file = fs::File::create(&temp_path).map_err(|err| Error::PreflightFailed {
        message: format!(
            "Failed to create download target {}: {err}",
            temp_path.display()
        ),
    })?;
    let mut hasher = Sha512::new();
    let mut total = 0u64;
    let mut buffer = [0u8; 64 * 1024];

    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|err| Error::PreflightFailed {
                message: format!(
                    "Failed while downloading {DEFAULT_ALPINE_URL}: {err}. \
                     Verify network connectivity or supply an explicit `base_image`."
                ),
            })?;
        if read == 0 {
            break;
        }
        file.write_all(&buffer[..read])
            .map_err(|err| Error::PreflightFailed {
                message: format!(
                    "Failed to write cached Alpine image {}: {err}",
                    temp_path.display()
                ),
            })?;
        hasher.update(&buffer[..read]);
        total += read as u64;
    }

    file.flush().map_err(|err| Error::PreflightFailed {
        message: format!(
            "Failed to flush cached Alpine image {}: {err}",
            temp_path.display()
        ),
    })?;

    if total != DEFAULT_ALPINE_SIZE_BYTES {
        let _ = fs::remove_file(&temp_path);
        return Err(Error::PreflightFailed {
            message: format!(
                "Downloaded Alpine base image has size {} bytes but expected {} bytes. \
                 Try running `castra clean --workspace` and retry, or set an explicit `base_image`.",
                total, DEFAULT_ALPINE_SIZE_BYTES
            ),
        });
    }

    let digest = hex::encode(hasher.finalize());
    if !digest.eq_ignore_ascii_case(DEFAULT_ALPINE_SHA512) {
        let _ = fs::remove_file(&temp_path);
        return Err(Error::PreflightFailed {
            message: format!(
                "Downloaded Alpine base image checksum mismatch (got {digest}). \
                 Remove {} and retry, or configure `base_image` manually.",
                target.display()
            ),
        });
    }

    fs::rename(&temp_path, target).map_err(|err| Error::PreflightFailed {
        message: format!(
            "Failed to finalize Alpine base image download to {}: {err}",
            target.display()
        ),
    })?;

    Ok(digest)
}

pub fn launch_vm(
    vm: &VmDefinition,
    assets: &ResolvedVmAssets,
    context: &RuntimeContext,
    events: &mut Vec<Event>,
) -> Result<u32> {
    let pidfile = context.state_root.join(format!("{}.pid", vm.name));
    if pidfile.exists() {
        let _ = fs::remove_file(&pidfile);
    }

    #[cfg(unix)]
    let qmp_socket = {
        let path = qmp_socket_path(&context.state_root, &vm.name);
        if path.exists() {
            let _ = fs::remove_file(&path);
        }
        path
    };

    let log_path = context.log_root.join(format!("{}.log", vm.name));
    let log_file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(|err| Error::LaunchFailed {
            vm: vm.name.clone(),
            message: format!("Could not open log file {}: {err}", log_path.display()),
        })?;
    let log_clone = log_file.try_clone().map_err(|err| Error::LaunchFailed {
        vm: vm.name.clone(),
        message: format!(
            "Could not duplicate log handle for {}: {err}",
            log_path.display()
        ),
    })?;

    let serial_path = context.log_root.join(format!("{}-serial.log", vm.name));
    fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&serial_path)
        .map_err(|err| Error::LaunchFailed {
            vm: vm.name.clone(),
            message: format!(
                "Could not prepare serial log {}: {err}",
                serial_path.display()
            ),
        })?;

    let memory_mib = vm
        .memory
        .bytes()
        .map(|bytes| cmp::max(1, (bytes / (1024 * 1024)) as u32))
        .unwrap_or(2048);

    let netdev = build_netdev_args(&vm.port_forwards);
    let drive_arg = format!(
        "file={},if=virtio,cache=writeback,format=qcow2",
        vm.overlay.display()
    );

    let mut command = Command::new(&context.qemu_system);
    command
        .arg("-name")
        .arg(&vm.name)
        .arg("-daemonize")
        .arg("-pidfile")
        .arg(&pidfile)
        .arg("-smp")
        .arg(vm.cpus.to_string())
        .arg("-m")
        .arg(format!("{memory_mib}M"))
        .arg("-drive")
        .arg(&drive_arg)
        .arg("-netdev")
        .arg(&netdev)
        .arg("-device")
        .arg("virtio-net-pci,netdev=castra-net0")
        .arg("-display")
        .arg("none")
        .arg("-serial")
        .arg(format!("file:{}", serial_path.display()))
        .stdout(Stdio::from(log_file))
        .stderr(Stdio::from(log_clone));

    #[cfg(unix)]
    {
        let qmp_arg = format!("unix:{},server=on,wait=off", qmp_socket.display());
        command.arg("-qmp").arg(qmp_arg);
    }

    let hvf_available = context.accelerators.iter().any(|accel| accel == "hvf");
    let kvm_available = context.accelerators.iter().any(|accel| accel == "kvm");

    let mut machine_has_accel = false;
    let mut hardware_accel = false;
    if let Some(boot) = &assets.boot {
        if let Some(machine) = &boot.machine {
            command.arg("-machine").arg(machine);
            machine_has_accel = machine.contains("accel=");
            hardware_accel |= accelerator_requested(&context.accelerators, machine);
        }
    }

    if cfg!(target_os = "macos") && hvf_available && !machine_has_accel {
        command.arg("-accel").arg("hvf");
        hardware_accel = true;
    }

    if cfg!(target_os = "linux") && kvm_available && !machine_has_accel {
        command.arg("-accel").arg("kvm");
        hardware_accel = true;
    }

    if let Some(boot) = &assets.boot {
        command.arg("-kernel").arg(&boot.kernel);
        if let Some(initrd) = &boot.initrd {
            command.arg("-initrd").arg(initrd);
        }
        if !boot.append.trim().is_empty() {
            command.arg("-append").arg(&boot.append);
        }
        for extra in &boot.extra_args {
            command.arg(extra);
        }
    }

    if hardware_accel {
        command.arg("-cpu").arg("host");
    }

    let status = command.status().map_err(|err| Error::LaunchFailed {
        vm: vm.name.clone(),
        message: format!("Failed to spawn {}: {err}", context.qemu_system.display()),
    })?;

    if !status.success() {
        return Err(Error::LaunchFailed {
            vm: vm.name.clone(),
            message: format!(
                "{} exited with status {}.",
                context.qemu_system.display(),
                status.code().unwrap_or(-1)
            ),
        });
    }

    wait_for_pidfile(&pidfile, Duration::from_secs(5)).map_err(|err| Error::LaunchFailed {
        vm: vm.name.clone(),
        message: format!(
            "QEMU did not write pidfile {} within timeout: {err}",
            pidfile.display()
        ),
    })?;

    let pid_contents = fs::read_to_string(&pidfile).map_err(|err| Error::LaunchFailed {
        vm: vm.name.clone(),
        message: format!(
            "Unable to read pidfile {} for VM `{}`: {err}",
            pidfile.display(),
            vm.name
        ),
    })?;
    let pid_trimmed = pid_contents.trim();
    let pid: u32 = pid_trimmed.parse().map_err(|_| Error::LaunchFailed {
        vm: vm.name.clone(),
        message: format!(
            "Pidfile {} for VM `{}` contained invalid pid `{pid_trimmed}`.",
            pidfile.display(),
            vm.name
        ),
    })?;

    events.push(Event::VmLaunched {
        vm: vm.name.clone(),
        pid,
    });

    Ok(pid)
}

/// Shutdown timing configuration derived from lifecycle settings and CLI overrides.
#[derive(Debug, Clone, Copy)]
pub struct ShutdownTimeouts {
    /// Duration to wait for cooperative shutdown before escalating.
    pub cooperative: Duration,
    /// Duration to wait after sending SIGTERM.
    pub sigterm: Duration,
    /// Duration to wait after sending SIGKILL.
    pub sigkill: Duration,
}

impl ShutdownTimeouts {
    /// Construct a new set of shutdown timeouts.
    pub fn new(cooperative: Duration, sigterm: Duration, sigkill: Duration) -> Self {
        Self {
            cooperative,
            sigterm,
            sigkill,
        }
    }
}

/// Aggregated result of a VM shutdown attempt.
#[derive(Debug)]
pub struct VmShutdownReport {
    /// Events emitted during the shutdown sequence.
    pub events: Vec<Event>,
    /// Diagnostics captured while processing the shutdown.
    pub diagnostics: Vec<Diagnostic>,
    /// Whether the VM transitioned from running to stopped.
    pub changed: bool,
    /// Final outcome (graceful vs forced).
    pub outcome: ShutdownOutcome,
}

impl VmShutdownReport {
    fn new(
        events: Vec<Event>,
        diagnostics: Vec<Diagnostic>,
        changed: bool,
        outcome: ShutdownOutcome,
    ) -> Self {
        Self {
            events,
            diagnostics,
            changed,
            outcome,
        }
    }
}

pub fn shutdown_vm(
    vm: &VmDefinition,
    state_root: &Path,
    timeouts: ShutdownTimeouts,
    event_tx: Option<&Sender<Event>>,
) -> Result<VmShutdownReport> {
    let mut events = Vec::new();
    let mut diagnostics = Vec::new();
    let mut emit_event = |event: Event| {
        if let Some(tx) = event_tx {
            let _ = tx.send(event.clone());
        }
        events.push(event);
    };

    let shutdown_started = Instant::now();
    emit_event(Event::ShutdownRequested {
        vm: vm.name.clone(),
    });

    let pidfile = state_root.join(format!("{}.pid", vm.name));
    if !pidfile.is_file() {
        cleanup_qmp_socket(state_root, &vm.name);
        let total_ms = duration_to_millis(shutdown_started.elapsed());
        emit_event(Event::ShutdownComplete {
            vm: vm.name.clone(),
            outcome: ShutdownOutcome::Graceful,
            total_ms,
            changed: false,
        });
        cleanup_ephemeral_layer(
            vm,
            &mut events,
            &mut diagnostics,
            EphemeralCleanupReason::Orphan,
        );
        return Ok(VmShutdownReport::new(
            events,
            diagnostics,
            false,
            ShutdownOutcome::Graceful,
        ));
    }

    let contents = fs::read_to_string(&pidfile).map_err(|err| Error::ShutdownFailed {
        vm: vm.name.clone(),
        message: format!(
            "Unable to read pidfile {} for VM `{}`: {err}",
            pidfile.display(),
            vm.name
        ),
    })?;

    let trimmed = contents.trim();
    if trimmed.is_empty() {
        diagnostics.push(Diagnostic::new(
            Severity::Warning,
            format!(
                "Pidfile {} for VM `{}` was empty; removing it.",
                pidfile.display(),
                vm.name
            ),
        ));
        let _ = fs::remove_file(&pidfile);
        cleanup_qmp_socket(state_root, &vm.name);
        let total_ms = duration_to_millis(shutdown_started.elapsed());
        emit_event(Event::ShutdownComplete {
            vm: vm.name.clone(),
            outcome: ShutdownOutcome::Graceful,
            total_ms,
            changed: false,
        });
        cleanup_ephemeral_layer(
            vm,
            &mut events,
            &mut diagnostics,
            EphemeralCleanupReason::Orphan,
        );
        return Ok(VmShutdownReport::new(
            events,
            diagnostics,
            false,
            ShutdownOutcome::Graceful,
        ));
    }

    let pid: pid_t = trimmed.parse().map_err(|_| Error::ShutdownFailed {
        vm: vm.name.clone(),
        message: format!(
            "Pidfile {} for VM `{}` contained invalid pid `{trimmed}`.",
            pidfile.display(),
            vm.name
        ),
    })?;

    let graceful_wait = timeouts.cooperative;
    let sigterm_wait = timeouts.sigterm;
    let sigkill_wait = timeouts.sigkill;
    let graceful_wait_ms = duration_to_millis(graceful_wait);
    let sigterm_wait_ms = duration_to_millis(sigterm_wait);
    let sigkill_wait_ms = duration_to_millis(sigkill_wait);

    let mut cooperative_available = false;
    let mut unavailable_detail: Option<String> = None;
    #[cfg(unix)]
    {
        let socket = qmp_socket_path(state_root, &vm.name);
        if socket.exists() {
            cooperative_available = true;
        } else {
            unavailable_detail = Some(format!(
                "QMP socket {} not available for `{}`",
                socket.display(),
                vm.name
            ));
        }
    }
    #[cfg(not(unix))]
    {
        unavailable_detail =
            Some("cooperative shutdown not supported on this platform".to_string());
    }

    let cooperative_method = if cooperative_available {
        CooperativeMethod::Acpi
    } else {
        CooperativeMethod::Unavailable
    };
    let cooperative_timeout_ms = if cooperative_available {
        graceful_wait_ms
    } else {
        0
    };

    emit_event(Event::CooperativeAttempted {
        vm: vm.name.clone(),
        method: cooperative_method,
        timeout_ms: cooperative_timeout_ms,
    });

    if !cooperative_available {
        emit_event(Event::CooperativeTimedOut {
            vm: vm.name.clone(),
            waited_ms: 0,
            reason: CooperativeTimeoutReason::ChannelUnavailable,
            detail: unavailable_detail.clone(),
        });
    } else {
        let _ = unavailable_detail;
        let graceful_attempt = attempt_graceful_shutdown(state_root, &vm.name);
        match graceful_attempt {
            GracefulTrigger::Initiated => {
                let wait_started = Instant::now();
                if wait_for_process_exit(pid, graceful_wait).map_err(|err| {
                    Error::ShutdownFailed {
                        vm: vm.name.clone(),
                        message: format!(
                            "Error while waiting for pid {pid} during graceful shutdown: {err}"
                        ),
                    }
                })? {
                    if let Err(err) = fs::remove_file(&pidfile) {
                        if err.kind() != ErrorKind::NotFound {
                            return Err(Error::ShutdownFailed {
                                vm: vm.name.clone(),
                                message: format!(
                                    "VM stopped but failed to remove pidfile {}: {err}",
                                    pidfile.display()
                                ),
                            });
                        }
                    }
                    cleanup_qmp_socket(state_root, &vm.name);
                    let elapsed_ms = duration_to_millis(wait_started.elapsed());
                    emit_event(Event::CooperativeSucceeded {
                        vm: vm.name.clone(),
                        elapsed_ms,
                    });
                    let total_ms = duration_to_millis(shutdown_started.elapsed());
                    emit_event(Event::ShutdownComplete {
                        vm: vm.name.clone(),
                        outcome: ShutdownOutcome::Graceful,
                        total_ms,
                        changed: true,
                    });
                    cleanup_ephemeral_layer(
                        vm,
                        &mut events,
                        &mut diagnostics,
                        EphemeralCleanupReason::Shutdown,
                    );
                    return Ok(VmShutdownReport::new(
                        events,
                        diagnostics,
                        true,
                        ShutdownOutcome::Graceful,
                    ));
                }
                emit_event(Event::CooperativeTimedOut {
                    vm: vm.name.clone(),
                    waited_ms: graceful_wait_ms,
                    reason: CooperativeTimeoutReason::TimeoutExpired,
                    detail: None,
                });
            }
            GracefulTrigger::Unavailable { detail } => {
                emit_event(Event::CooperativeTimedOut {
                    vm: vm.name.clone(),
                    waited_ms: 0,
                    reason: CooperativeTimeoutReason::ChannelUnavailable,
                    detail: Some(detail),
                });
            }
            GracefulTrigger::Failed { detail } => {
                diagnostics.push(
                    Diagnostic::new(
                        Severity::Warning,
                        format!("Failed graceful shutdown for VM `{}`: {detail}", vm.name),
                    )
                    .with_help(
                        "Ensure QEMU launched with QMP support or allow Castra to manage the VM lifecycle.",
                    ),
                );
                emit_event(Event::CooperativeTimedOut {
                    vm: vm.name.clone(),
                    waited_ms: 0,
                    reason: CooperativeTimeoutReason::ChannelError,
                    detail: Some(detail),
                });
            }
        }
    }

    let term = unsafe { libc::kill(pid, libc::SIGTERM) };
    if term != 0 {
        let errno = io::Error::last_os_error()
            .raw_os_error()
            .unwrap_or_default();
        if errno == libc::ESRCH {
            diagnostics.push(Diagnostic::new(
                Severity::Warning,
                format!(
                    "Pidfile {} for VM `{}` was stale (process {pid} already exited); removing.",
                    pidfile.display(),
                    vm.name
                ),
            ));
            let _ = fs::remove_file(&pidfile);
            cleanup_qmp_socket(state_root, &vm.name);
            let total_ms = duration_to_millis(shutdown_started.elapsed());
            emit_event(Event::ShutdownComplete {
                vm: vm.name.clone(),
                outcome: ShutdownOutcome::Graceful,
                total_ms,
                changed: false,
            });
            cleanup_ephemeral_layer(
                vm,
                &mut events,
                &mut diagnostics,
                EphemeralCleanupReason::Orphan,
            );
            return Ok(VmShutdownReport::new(
                events,
                diagnostics,
                false,
                ShutdownOutcome::Graceful,
            ));
        }

        return Err(Error::ShutdownFailed {
            vm: vm.name.clone(),
            message: format!("Failed to send SIGTERM to pid {pid}: errno {errno}"),
        });
    }

    emit_event(Event::ShutdownEscalated {
        vm: vm.name.clone(),
        signal: ShutdownSignal::Sigterm,
        timeout_ms: Some(sigterm_wait_ms),
    });
    let outcome = ShutdownOutcome::Forced;
    if !wait_for_process_exit(pid, sigterm_wait).map_err(|err| Error::ShutdownFailed {
        vm: vm.name.clone(),
        message: format!("Error while waiting for pid {pid} to exit: {err}"),
    })? {
        let kill_res = unsafe { libc::kill(pid, libc::SIGKILL) };
        emit_event(Event::ShutdownEscalated {
            vm: vm.name.clone(),
            signal: ShutdownSignal::Sigkill,
            timeout_ms: Some(sigkill_wait_ms),
        });
        if kill_res != 0 {
            let errno = io::Error::last_os_error()
                .raw_os_error()
                .unwrap_or_default();
            if errno != libc::ESRCH {
                return Err(Error::ShutdownFailed {
                    vm: vm.name.clone(),
                    message: format!("Failed to send SIGKILL to pid {pid}: errno {errno}"),
                });
            }
        }

        if !wait_for_process_exit(pid, sigkill_wait).map_err(|err| Error::ShutdownFailed {
            vm: vm.name.clone(),
            message: format!("Error while waiting for pid {pid} after SIGKILL: {err}"),
        })? {
            return Err(Error::ShutdownFailed {
                vm: vm.name.clone(),
                message: format!("Process {pid} did not exit after SIGKILL."),
            });
        }
    }

    if let Err(err) = fs::remove_file(&pidfile) {
        if err.kind() != ErrorKind::NotFound {
            return Err(Error::ShutdownFailed {
                vm: vm.name.clone(),
                message: format!(
                    "VM stopped but failed to remove pidfile {}: {err}",
                    pidfile.display()
                ),
            });
        }
    }
    cleanup_qmp_socket(state_root, &vm.name);
    let total_ms = duration_to_millis(shutdown_started.elapsed());
    emit_event(Event::ShutdownComplete {
        vm: vm.name.clone(),
        outcome,
        total_ms,
        changed: true,
    });
    cleanup_ephemeral_layer(
        vm,
        &mut events,
        &mut diagnostics,
        EphemeralCleanupReason::Shutdown,
    );

    Ok(VmShutdownReport::new(events, diagnostics, true, outcome))
}

pub fn shutdown_broker(
    state_root: &Path,
    events: &mut Vec<Event>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<bool> {
    let pidfile = state_root.join("broker.pid");
    if !pidfile.is_file() {
        events.push(Event::BrokerStopped { changed: false });
        return Ok(false);
    }

    let contents = fs::read_to_string(&pidfile).map_err(|err| Error::ShutdownFailed {
        vm: "broker".to_string(),
        message: format!("Unable to read broker pidfile {}: {err}", pidfile.display()),
    })?;

    let trimmed = contents.trim();
    let pid: pid_t = trimmed.parse().map_err(|_| Error::ShutdownFailed {
        vm: "broker".to_string(),
        message: format!(
            "Broker pidfile {} contained invalid pid `{trimmed}`.",
            pidfile.display()
        ),
    })?;

    let term = unsafe { libc::kill(pid, libc::SIGTERM) };
    if term != 0 {
        let errno = io::Error::last_os_error()
            .raw_os_error()
            .unwrap_or_default();
        if errno == libc::ESRCH {
            diagnostics.push(Diagnostic::new(
                Severity::Warning,
                format!(
                    "Broker pidfile {} was stale (process {pid} already exited); removing.",
                    pidfile.display()
                ),
            ));
            let _ = fs::remove_file(&pidfile);
            events.push(Event::BrokerStopped { changed: false });
            return Ok(false);
        }

        return Err(Error::ShutdownFailed {
            vm: "broker".to_string(),
            message: format!("Failed to send SIGTERM to broker pid {pid}: errno {errno}"),
        });
    }

    events.push(Event::Message {
        severity: Severity::Info,
        text: format!("→ broker: sent SIGTERM to pid {}.", pid),
    });
    if !wait_for_process_exit(pid, Duration::from_secs(5)).map_err(|err| Error::ShutdownFailed {
        vm: "broker".to_string(),
        message: format!("Error while waiting for broker pid {pid}: {err}"),
    })? {
        events.push(Event::Message {
            severity: Severity::Warning,
            text: format!("→ broker: escalating to SIGKILL (pid {}).", pid),
        });
        let kill_res = unsafe { libc::kill(pid, libc::SIGKILL) };
        if kill_res != 0 {
            let errno = io::Error::last_os_error()
                .raw_os_error()
                .unwrap_or_default();
            if errno != libc::ESRCH {
                return Err(Error::ShutdownFailed {
                    vm: "broker".to_string(),
                    message: format!("Failed to send SIGKILL to broker pid {pid}: errno {errno}"),
                });
            }
        }

        if !wait_for_process_exit(pid, Duration::from_secs(5)).map_err(|err| {
            Error::ShutdownFailed {
                vm: "broker".to_string(),
                message: format!("Error while waiting for broker pid {pid} after SIGKILL: {err}"),
            }
        })? {
            return Err(Error::ShutdownFailed {
                vm: "broker".to_string(),
                message: "Broker process did not exit after SIGKILL.".to_string(),
            });
        }
    }

    if let Err(err) = fs::remove_file(&pidfile) {
        if err.kind() != io::ErrorKind::NotFound {
            return Err(Error::ShutdownFailed {
                vm: "broker".to_string(),
                message: format!(
                    "Broker stopped but failed to remove pidfile {}: {err}",
                    pidfile.display()
                ),
            });
        }
    }

    events.push(Event::BrokerStopped { changed: true });
    Ok(true)
}

pub(crate) fn inspect_vm_state(
    pidfile: &Path,
    vm_name: &str,
) -> (String, Option<Duration>, Vec<String>) {
    let mut warnings = Vec::new();

    if !pidfile.is_file() {
        return ("stopped".to_string(), None, warnings);
    }

    let contents = match fs::read_to_string(pidfile) {
        Ok(contents) => contents,
        Err(err) => {
            warnings.push(format!(
                "Unable to read pidfile for VM `{vm_name}` at {}: {err}",
                pidfile.display()
            ));
            return ("unknown".to_string(), None, warnings);
        }
    };

    let trimmed = contents.trim();
    if trimmed.is_empty() {
        warnings.push(format!(
            "Pidfile for VM `{vm_name}` at {} is empty. Removing stale file.",
            pidfile.display()
        ));
        let _ = fs::remove_file(pidfile);
        return ("stopped".to_string(), None, warnings);
    }

    let pid: pid_t = match trimmed.parse() {
        Ok(pid) => pid,
        Err(_) => {
            warnings.push(format!(
                "Pidfile for VM `{vm_name}` at {} has invalid contents `{trimmed}`. Removing stale file.",
                pidfile.display()
            ));
            let _ = fs::remove_file(pidfile);
            return ("stopped".to_string(), None, warnings);
        }
    };

    let alive = unsafe { libc::kill(pid, 0) };
    if alive == 0 {
        let uptime = uptime_from_pidfile(pidfile);
        return ("running".to_string(), uptime, warnings);
    }

    let errno = io::Error::last_os_error()
        .raw_os_error()
        .unwrap_or_default();
    if errno == libc::EPERM {
        let uptime = uptime_from_pidfile(pidfile);
        return ("running".to_string(), uptime, warnings);
    }

    if errno == libc::ESRCH {
        warnings.push(format!(
            "Removing stale pidfile for VM `{vm_name}` at {} (process {pid} no longer exists).",
            pidfile.display()
        ));
        if let Err(err) = fs::remove_file(pidfile) {
            warnings.push(format!(
                "Failed to remove stale pidfile for VM `{vm_name}` at {}: {err}",
                pidfile.display()
            ));
        }
        return ("stopped".to_string(), None, warnings);
    }

    warnings.push(format!(
        "Unable to determine state for VM `{vm_name}` (pid {pid}, errno {errno}).",
        errno = errno,
        pid = pid
    ));
    ("unknown".to_string(), None, warnings)
}

pub(crate) fn inspect_broker_state(pidfile: &Path) -> (BrokerProcessState, Vec<String>) {
    let mut warnings = Vec::new();

    if !pidfile.is_file() {
        return (BrokerProcessState::Offline, warnings);
    }

    let contents = match fs::read_to_string(pidfile) {
        Ok(contents) => contents,
        Err(err) => {
            warnings.push(format!(
                "Unable to read broker pidfile at {}: {err}",
                pidfile.display()
            ));
            return (BrokerProcessState::Offline, warnings);
        }
    };

    let trimmed = contents.trim();
    if trimmed.is_empty() {
        warnings.push(format!(
            "Broker pidfile at {} is empty. Removing stale file.",
            pidfile.display()
        ));
        let _ = fs::remove_file(pidfile);
        return (BrokerProcessState::Offline, warnings);
    }

    let pid: pid_t = match trimmed.parse() {
        Ok(pid) => pid,
        Err(_) => {
            warnings.push(format!(
                "Broker pidfile at {} has invalid pid `{trimmed}`. Removing stale file.",
                pidfile.display()
            ));
            let _ = fs::remove_file(pidfile);
            return (BrokerProcessState::Offline, warnings);
        }
    };

    let alive = unsafe { libc::kill(pid, 0) };
    if alive == 0 {
        return (BrokerProcessState::Running { pid }, warnings);
    }

    let errno = io::Error::last_os_error()
        .raw_os_error()
        .unwrap_or_default();
    if errno == libc::EPERM {
        return (BrokerProcessState::Running { pid }, warnings);
    }

    if errno == libc::ESRCH {
        warnings.push(format!(
            "Removing stale broker pidfile at {} (process {pid} no longer exists).",
            pidfile.display()
        ));
        if let Err(err) = fs::remove_file(pidfile) {
            warnings.push(format!(
                "Failed to remove stale broker pidfile at {}: {err}",
                pidfile.display()
            ));
        }
        return (BrokerProcessState::Offline, warnings);
    }

    warnings.push(format!(
        "Unable to determine broker process state (pid {pid}, errno {errno}).",
        errno = errno,
        pid = pid
    ));
    (BrokerProcessState::Offline, warnings)
}

fn ensure_port_is_free(port: u16, description: &str) -> Result<()> {
    let bind_addr = format!("127.0.0.1:{port}");
    match TcpListener::bind(&bind_addr) {
        Ok(listener) => {
            drop(listener);
            Ok(())
        }
        Err(err) if err.kind() == io::ErrorKind::AddrInUse => Err(Error::PreflightFailed {
            message: format!(
                "Host port {port} ({description}) is already in use. Stop the conflicting service or change the port in castra.toml."
            ),
        }),
        Err(err) => Err(Error::PreflightFailed {
            message: format!("Unable to check host port {port} for {description}: {err}"),
        }),
    }
}

fn broker_pid_path(context: &RuntimeContext) -> PathBuf {
    context.state_root.join("broker.pid")
}

fn broker_log_path(context: &RuntimeContext) -> PathBuf {
    context.log_root.join("broker.log")
}

fn broker_handshake_dir(context: &RuntimeContext) -> PathBuf {
    broker_handshake_dir_from_root(&context.state_root)
}

pub(crate) fn broker_handshake_dir_from_root(state_root: &Path) -> PathBuf {
    state_root.join("handshakes")
}

#[derive(Debug)]
enum GracefulTrigger {
    Initiated,
    Unavailable { detail: String },
    Failed { detail: String },
}

fn attempt_graceful_shutdown(state_root: &Path, vm_name: &str) -> GracefulTrigger {
    #[cfg(unix)]
    {
        let socket = qmp_socket_path(state_root, vm_name);
        match send_qmp_powerdown(&socket) {
            Ok(()) => GracefulTrigger::Initiated,
            Err(GracefulShutdownError::Unavailable { detail }) => {
                let base = format!(
                    "QMP socket {} not available for `{vm_name}`",
                    socket.display(),
                    vm_name = vm_name
                );
                let detail = match detail {
                    Some(extra) if !extra.is_empty() => format!("{base} ({extra})"),
                    _ => base,
                };
                GracefulTrigger::Unavailable { detail }
            }
            Err(GracefulShutdownError::Io(err)) => GracefulTrigger::Failed {
                detail: format!(
                    "QMP connection error via {} while powering down `{vm_name}`: {err}",
                    socket.display(),
                    vm_name = vm_name
                ),
            },
            Err(GracefulShutdownError::Protocol(reason)) => {
                GracefulTrigger::Failed { detail: reason }
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = (state_root, vm_name);
        GracefulTrigger::Unavailable {
            detail: "cooperative shutdown not supported on this platform".to_string(),
        }
    }
}

fn cleanup_qmp_socket(state_root: &Path, vm_name: &str) {
    #[cfg(unix)]
    {
        let socket = qmp_socket_path(state_root, vm_name);
        if socket.exists() {
            let _ = fs::remove_file(socket);
        }
    }
    #[cfg(not(unix))]
    {
        let _ = (state_root, vm_name);
    }
}

#[cfg(unix)]
fn qmp_socket_path(state_root: &Path, vm_name: &str) -> PathBuf {
    state_root.join(format!("{}.qmp", vm_name))
}

#[cfg(unix)]
enum GracefulShutdownError {
    Unavailable { detail: Option<String> },
    Io(io::Error),
    Protocol(String),
}

fn send_qmp_powerdown(socket: &Path) -> std::result::Result<(), GracefulShutdownError> {
    if !socket.exists() {
        return Err(GracefulShutdownError::Unavailable { detail: None });
    }

    let mut stream = UnixStream::connect(&socket).map_err(map_qmp_connect_error)?;
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .map_err(GracefulShutdownError::Io)?;
    stream
        .set_write_timeout(Some(Duration::from_secs(2)))
        .map_err(GracefulShutdownError::Io)?;
    let mut reader = BufReader::new(stream.try_clone().map_err(GracefulShutdownError::Io)?);

    let message = read_qmp_message(&mut reader)?;
    if message.get("QMP").is_none() {
        return Err(GracefulShutdownError::Protocol(format!(
            "Unexpected QMP greeting from {}",
            socket.display()
        )));
    }

    send_qmp_command(&mut stream, "qmp_capabilities")?;
    wait_for_qmp_ok(&mut reader)?;
    send_qmp_command(&mut stream, "system_powerdown")?;
    wait_for_qmp_ok(&mut reader)?;
    Ok(())
}

#[cfg(unix)]
fn map_qmp_connect_error(err: io::Error) -> GracefulShutdownError {
    match err.kind() {
        io::ErrorKind::NotFound
        | io::ErrorKind::ConnectionRefused
        | io::ErrorKind::AddrNotAvailable
        | io::ErrorKind::PermissionDenied => GracefulShutdownError::Unavailable {
            detail: Some(err.to_string()),
        },
        _ => GracefulShutdownError::Io(err),
    }
}

#[cfg(unix)]
fn send_qmp_command(
    stream: &mut UnixStream,
    command: &str,
) -> std::result::Result<(), GracefulShutdownError> {
    let payload = json!({ "execute": command });
    let mut data = serde_json::to_string(&payload)
        .map_err(|err| GracefulShutdownError::Protocol(err.to_string()))?;
    data.push('\n');
    stream
        .write_all(data.as_bytes())
        .map_err(GracefulShutdownError::Io)
}

#[cfg(unix)]
fn read_qmp_message(
    reader: &mut BufReader<UnixStream>,
) -> std::result::Result<Value, GracefulShutdownError> {
    let mut line = String::new();
    let bytes = reader
        .read_line(&mut line)
        .map_err(GracefulShutdownError::Io)?;
    if bytes == 0 {
        return Err(GracefulShutdownError::Unavailable {
            detail: Some("QMP connection closed unexpectedly.".to_string()),
        });
    }
    serde_json::from_str(&line).map_err(|err| GracefulShutdownError::Protocol(err.to_string()))
}

#[cfg(unix)]
fn wait_for_qmp_ok(
    reader: &mut BufReader<UnixStream>,
) -> std::result::Result<(), GracefulShutdownError> {
    loop {
        let message = read_qmp_message(reader)?;
        if message.get("return").is_some() {
            return Ok(());
        }
        if let Some(err) = message.get("error") {
            return Err(GracefulShutdownError::Protocol(format!(
                "QMP error response: {}",
                err
            )));
        }
        // Ignore asynchronous events.
    }
}

fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    if bytes == 0 {
        return "0 B".to_string();
    }

    let mut value = bytes as f64;
    let mut unit = 0usize;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }

    if unit == 0 {
        format!("{value:.0} {}", UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

fn existing_directory(path: &Path) -> PathBuf {
    let mut cursor = Some(path);
    while let Some(dir) = cursor {
        if dir.is_dir() {
            return fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
        }
        cursor = dir.parent();
    }

    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

fn available_disk_space(disks: &mut Disks, path: &Path) -> Option<u64> {
    let canonical = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let mut best: Option<(u64, usize)> = None;

    for disk in disks.iter_mut() {
        let mount = disk.mount_point().to_path_buf();
        if canonical.starts_with(&mount) {
            disk.refresh();
            let depth = mount.components().count();
            let take = match best {
                Some((_, best_depth)) => depth > best_depth,
                None => true,
            };
            if take {
                best = Some((disk.available_space(), depth));
            }
        }
    }

    best.map(|(space, _)| space)
}

fn ensure_overlay(
    vm: &VmDefinition,
    context: &RuntimeContext,
    base_disk: &Path,
) -> Result<(bool, Option<u64>)> {
    if let Some(parent) = vm.overlay.parent() {
        fs::create_dir_all(parent).map_err(|err| Error::PreflightFailed {
            message: format!(
                "Failed to prepare overlay directory {} for VM `{}`: {err}.",
                parent.display(),
                vm.name
            ),
        })?;
    }

    let Some(qemu_img) = &context.qemu_img else {
        return Err(Error::PreflightFailed {
            message: format!(
                "Ephemeral storage for VM `{}` requires `qemu-img` but it was not found in PATH. Install QEMU tooling (e.g. `brew install qemu` or `sudo apt install qemu-utils`) before running `castra up` again.",
                vm.name,
            ),
        });
    };

    let reclaimed = match discard_overlay_file(&vm.overlay) {
        Ok(result) => result,
        Err(err) => {
            return Err(Error::PreflightFailed {
                message: format!(
                    "Failed to remove stale ephemeral overlay {} for VM `{}`: {err}. Clean it manually (`rm {}`) and retry.",
                    vm.overlay.display(),
                    vm.name,
                    vm.overlay.display()
                ),
            });
        }
    };

    let base_format = detect_image_format(qemu_img, base_disk);
    create_overlay(
        qemu_img,
        base_disk,
        &vm.overlay,
        &vm.name,
        base_format.as_deref(),
    )?;
    Ok((true, reclaimed))
}

fn create_overlay(
    qemu_img: &Path,
    base: &Path,
    overlay: &Path,
    vm_name: &str,
    base_format: Option<&str>,
) -> Result<()> {
    let mut command = Command::new(qemu_img);
    command.arg("create").arg("-f").arg("qcow2");
    if let Some(format) = base_format {
        command.arg("-F").arg(format);
    }
    let status = command
        .arg("-b")
        .arg(base)
        .arg(overlay)
        .status()
        .map_err(|err| Error::PreflightFailed {
            message: format!(
                "Failed to invoke `{}` while creating overlay for VM `{vm_name}`: {err}",
                qemu_img.display()
            ),
        })?;

    if !status.success() {
        return Err(Error::PreflightFailed {
            message: format!(
                "`{}` exited with code {} while preparing overlay {} for VM `{vm_name}`.",
                qemu_img.display(),
                status.code().unwrap_or(-1),
                overlay.display()
            ),
        });
    }

    Ok(())
}

fn detect_image_format(qemu_img: &Path, image: &Path) -> Option<String> {
    let output = Command::new(qemu_img)
        .arg("info")
        .arg("--output=json")
        .arg(image)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let value: Value = serde_json::from_slice(&output.stdout).ok()?;
    if let Some(obj) = value.as_object() {
        obj.get("format")
            .and_then(Value::as_str)
            .map(str::to_string)
    } else if let Some(first) = value.as_array().and_then(|array| array.first()) {
        first
            .as_object()
            .and_then(|obj| obj.get("format"))
            .and_then(Value::as_str)
            .map(str::to_string)
    } else {
        None
    }
}

fn discard_overlay_file(path: &Path) -> io::Result<Option<u64>> {
    match fs::metadata(path) {
        Ok(metadata) => {
            let bytes = metadata.len();
            fs::remove_file(path)?;
            Ok(Some(bytes))
        }
        Err(err) if err.kind() == ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err),
    }
}

fn cleanup_ephemeral_layer(
    vm: &VmDefinition,
    events: &mut Vec<Event>,
    diagnostics: &mut Vec<Diagnostic>,
    reason: EphemeralCleanupReason,
) {
    match discard_overlay_file(&vm.overlay) {
        Ok(Some(bytes)) => {
            events.push(Event::EphemeralLayerDiscarded {
                vm: vm.name.clone(),
                overlay_path: vm.overlay.clone(),
                reclaimed_bytes: bytes,
                reason,
            });
        }
        Ok(None) => {}
        Err(err) => {
            diagnostics.push(
                Diagnostic::new(
                    Severity::Warning,
                    format!(
                        "Failed to remove ephemeral overlay {} for VM `{}`: {err}",
                        vm.overlay.display(),
                        vm.name
                    ),
                )
                .with_help("Remove it manually or run `castra clean --include-overlays`."),
            );
        }
    }
}

fn detect_available_accelerators(qemu_system: &Path) -> Vec<String> {
    let output = Command::new(qemu_system)
        .arg("-accel")
        .arg("help")
        .output()
        .ok();

    let stdout = match output {
        Some(output) if output.status.success() => output.stdout,
        _ => return Vec::new(),
    };

    let mut set = HashSet::new();
    for line in String::from_utf8_lossy(&stdout).lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("Accelerators supported") {
            continue;
        }
        if let Some(token) = trimmed.split_whitespace().next() {
            set.insert(token.to_lowercase());
        }
    }

    let mut accelerators: Vec<String> = set.into_iter().collect();
    accelerators.sort();
    accelerators
}

fn accelerator_requested(available: &[String], machine: &str) -> bool {
    let accel_pos = match machine.find("accel=") {
        Some(idx) => idx + "accel=".len(),
        None => return false,
    };

    let remainder = &machine[accel_pos..];
    let accel_segment = remainder.split(',').next().unwrap_or(remainder);

    for candidate in accel_segment.split(':') {
        let lower = candidate.trim().to_lowercase();
        if lower.is_empty() {
            continue;
        }
        if lower == "tcg" {
            continue;
        }
        if available.iter().any(|value| value == &lower) {
            return true;
        }
    }

    false
}

fn build_netdev_args(forwards: &[PortForward]) -> String {
    let mut net = String::from("user,id=castra-net0");
    for forward in forwards {
        let proto = match forward.protocol {
            PortProtocol::Tcp => "tcp",
            PortProtocol::Udp => "udp",
        };
        net.push_str(",hostfwd=");
        net.push_str(proto);
        net.push_str("::");
        net.push_str(&forward.host.to_string());
        net.push_str("-:");
        net.push_str(&forward.guest.to_string());
    }
    net
}

fn duration_to_millis(duration: Duration) -> u64 {
    duration.as_millis().min(u128::from(u64::MAX)) as u64
}

fn wait_for_pidfile(pidfile: &Path, timeout: Duration) -> io::Result<()> {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if pidfile.exists() {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(100));
    }
    if pidfile.exists() {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "pidfile not created in time",
        ))
    }
}

fn wait_for_process_exit(pid: pid_t, timeout: Duration) -> io::Result<bool> {
    let start = Instant::now();
    loop {
        let res = unsafe { libc::kill(pid, 0) };
        if res == -1 {
            let errno = io::Error::last_os_error()
                .raw_os_error()
                .unwrap_or_default();
            if errno == libc::ESRCH {
                return Ok(true);
            }
        }

        if start.elapsed() >= timeout {
            return Ok(false);
        }

        thread::sleep(Duration::from_millis(200));
    }
}

fn uptime_from_pidfile(pidfile: &Path) -> Option<Duration> {
    let metadata = fs::metadata(pidfile).ok()?;
    let modified = metadata.modified().ok()?;
    SystemTime::now().duration_since(modified).ok()
}

fn find_executable(candidates: &[&str]) -> Option<PathBuf> {
    for candidate in candidates {
        let path = Path::new(candidate);
        if path.is_file() {
            return Some(path.to_path_buf());
        }
    }

    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        for candidate in candidates {
            let full = dir.join(candidate);
            if full.is_file() {
                return Some(full);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        BaseImageSource, BootstrapMode, DEFAULT_BOOTSTRAP_HANDSHAKE_WAIT_SECS, MemorySpec,
        VmBootstrapConfig, VmDefinition,
    };
    use crate::error::Error;
    use std::collections::HashMap;
    use std::fs;
    use std::net::TcpListener;
    use std::path::Path;
    use std::thread;
    use std::time::Duration;
    use tempfile::tempdir;

    #[test]
    fn assess_cache_reports_missing_image() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("alpine-minimal.qcow2");
        let status = assess_alpine_cache(&path).expect("cache check");
        match status {
            AlpineCacheStatus::NeedsDownload { reason } => {
                assert!(reason.contains("not found"));
            }
            other => panic!("unexpected status: {other:?}"),
        }
    }

    #[test]
    fn assess_cache_detects_size_mismatch() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("alpine-minimal.qcow2");
        fs::write(&path, b"stub").unwrap();
        let status = assess_alpine_cache(&path).expect("cache check");
        match status {
            AlpineCacheStatus::NeedsDownload { reason } => {
                assert!(reason.contains("expected"), "{reason}");
            }
            other => panic!("unexpected status: {other:?}"),
        }
    }

    #[test]
    fn format_bytes_formats_human_readable_values() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(1024), "1.0 KiB");
        assert_eq!(format_bytes(1024 * 1024), "1.0 MiB");
        assert_eq!(format_bytes(1536), "1.5 KiB");
    }

    #[test]
    fn existing_directory_returns_nearest_parent() {
        let dir = tempdir().unwrap();
        let nested = dir.path().join("a/b/c");
        let existing = existing_directory(&nested);
        assert_eq!(existing, fs::canonicalize(dir.path()).unwrap());
    }

    #[test]
    fn available_disk_space_reports_for_known_directory() {
        let dir = tempdir().unwrap();
        let mut disks = Disks::new_with_refreshed_list();
        let space = available_disk_space(&mut disks, dir.path());
        assert!(space.is_some());
        if let Some(bytes) = space {
            assert!(bytes > 0);
        }
    }

    #[test]
    fn ensure_port_is_free_detects_conflicts() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let err = ensure_port_is_free(port, "test").unwrap_err();
        match err {
            Error::PreflightFailed { message } => {
                assert!(message.contains("already in use"));
            }
            other => panic!("unexpected error: {other:?}"),
        }
        drop(listener);
        ensure_port_is_free(port, "test").expect("port should be free after drop");
    }

    #[test]
    fn build_netdev_args_formats_forwards() {
        let forwards = vec![
            PortForward {
                host: 2222,
                guest: 22,
                protocol: PortProtocol::Tcp,
            },
            PortForward {
                host: 8080,
                guest: 80,
                protocol: PortProtocol::Udp,
            },
        ];
        let args = build_netdev_args(&forwards);
        assert!(args.contains("user,id=castra-net0"));
        assert!(args.contains("hostfwd=tcp::2222-:22"));
        assert!(args.contains("hostfwd=udp::8080-:80"));
    }

    #[test]
    fn accelerator_requested_detects_available_accelerator() {
        let available = vec!["hvf".to_string(), "kvm".to_string()];
        assert!(accelerator_requested(&available, "accel=hvf:tcg"));
        assert!(!accelerator_requested(&available, "machine=q35"));
        assert!(!accelerator_requested(&available, "accel=tcg"));
    }

    #[cfg(unix)]
    fn sample_vm(state_root: &Path) -> VmDefinition {
        let base = state_root.join("base.img");
        fs::write(&base, b"base image").unwrap();
        let overlay = state_root.join("overlay.qcow2");
        fs::write(&overlay, b"overlay image").unwrap();
        VmDefinition {
            name: "devbox".to_string(),
            role_name: "devbox".to_string(),
            replica_index: 0,
            description: None,
            base_image: BaseImageSource::from_explicit(base),
            overlay,
            cpus: 1,
            memory: MemorySpec::new("512 MiB", Some(512_u64 * 1024 * 1024)),
            port_forwards: Vec::new(),
            bootstrap: VmBootstrapConfig {
                mode: BootstrapMode::Skip,
                script: None,
                payload: None,
                handshake_timeout_secs: DEFAULT_BOOTSTRAP_HANDSHAKE_WAIT_SECS,
                remote_dir: PathBuf::from("/tmp/castra-bootstrap"),
                env: HashMap::new(),
                verify: None,
            },
        }
    }

    #[cfg(unix)]
    struct ForkGuard {
        pid: libc::pid_t,
    }

    #[cfg(unix)]
    impl ForkGuard {
        fn spawn() -> Self {
            unsafe {
                let pid = libc::fork();
                if pid < 0 {
                    panic!("fork failed: {}", std::io::Error::last_os_error());
                }
                if pid == 0 {
                    loop {
                        libc::pause();
                    }
                }
                Self { pid }
            }
        }

        fn pid(&self) -> libc::pid_t {
            self.pid
        }
    }

    #[cfg(unix)]
    impl Drop for ForkGuard {
        fn drop(&mut self) {
            unsafe {
                let _ = libc::kill(self.pid, libc::SIGKILL);
                let mut status = 0;
                let _ = libc::waitpid(self.pid, &mut status, libc::WNOHANG);
            }
        }
    }

    #[cfg(unix)]
    #[test]
    fn cooperative_shutdown_reports_success_via_qmp()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        use serde_json::Value;
        use std::io::{BufRead, BufReader, Write};
        use std::os::unix::net::UnixListener;
        use std::thread;
        use std::time::Duration;

        let temp = tempdir()?;
        let state_root = temp.path().to_path_buf();
        let vm = sample_vm(&state_root);

        let child = ForkGuard::spawn();
        let pidfile = state_root.join(format!("{}.pid", vm.name));
        let pid_value = child.pid();
        fs::write(&pidfile, format!("{pid_value}"))?;
        let socket_path = state_root.join(format!("{}.qmp", vm.name));
        let listener = UnixListener::bind(&socket_path)?;
        let graceful_pid = pid_value;

        let reaper = thread::spawn(move || {
            loop {
                let mut status = 0;
                let res = unsafe { libc::waitpid(pid_value, &mut status, libc::WNOHANG) };
                if res == 0 {
                    thread::sleep(Duration::from_millis(20));
                    continue;
                }
                break;
            }
        });

        let qmp_thread = thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let greeting = r#"{"QMP":{"version":{"qemu":{"major":8,"minor":2,"micro":0}},"capabilities":[]}}"#;
                stream.write_all(greeting.as_bytes()).unwrap();
                stream.write_all(b"\n").unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap() == 0 {
                        break;
                    }
                    let value: Value = serde_json::from_str(&line).unwrap();
                    if let Some(cmd) = value.get("execute").and_then(|v| v.as_str()) {
                        stream.write_all(br#"{"return":{}}"#).unwrap();
                        stream.write_all(b"\n").unwrap();
                        if cmd == "system_powerdown" {
                            unsafe {
                                libc::kill(graceful_pid, libc::SIGTERM);
                            }
                            break;
                        }
                    }
                }
            }
        });

        let timeouts = ShutdownTimeouts::new(
            Duration::from_secs(2),
            Duration::from_millis(500),
            Duration::from_millis(500),
        );
        let report = shutdown_vm(&vm, &state_root, timeouts, None)?;
        qmp_thread.join().unwrap();
        reaper.join().unwrap();

        assert!(report.diagnostics.is_empty());
        assert!(report.changed);
        assert_eq!(report.outcome, ShutdownOutcome::Graceful);
        assert!(!pidfile.exists());

        let events = &report.events;
        assert_eq!(events.len(), 5, "unexpected event sequence: {:?}", events);
        match &events[0] {
            Event::ShutdownRequested { vm: event_vm } => assert_eq!(event_vm, &vm.name),
            other => panic!("expected ShutdownRequested event, got {other:?}"),
        }
        match &events[1] {
            Event::CooperativeAttempted {
                vm: event_vm,
                method,
                timeout_ms,
            } => {
                assert_eq!(event_vm, &vm.name);
                assert_eq!(*method, CooperativeMethod::Acpi);
                assert_eq!(*timeout_ms, 2000);
            }
            other => panic!("expected CooperativeAttempted event, got {other:?}"),
        }
        match &events[2] {
            Event::CooperativeSucceeded {
                vm: event_vm,
                elapsed_ms,
            } => {
                assert_eq!(event_vm, &vm.name);
                assert!(*elapsed_ms <= 2000);
            }
            other => panic!("expected CooperativeSucceeded event, got {other:?}"),
        }
        match &events[3] {
            Event::ShutdownComplete {
                vm: event_vm,
                outcome,
                changed,
                ..
            } => {
                assert_eq!(event_vm, &vm.name);
                assert_eq!(*outcome, ShutdownOutcome::Graceful);
                assert!(*changed);
            }
            other => panic!("expected ShutdownComplete event, got {other:?}"),
        }
        match &events[4] {
            Event::EphemeralLayerDiscarded {
                vm: event_vm,
                reason,
                ..
            } => {
                assert_eq!(event_vm, &vm.name);
                assert_eq!(*reason, EphemeralCleanupReason::Shutdown);
            }
            other => panic!("expected EphemeralLayerDiscarded event after shutdown, got {other:?}"),
        }

        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn cooperative_shutdown_handles_unavailable_qmp_channel()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let temp = tempdir()?;
        let state_root = temp.path().to_path_buf();
        let vm = sample_vm(&state_root);

        let child = ForkGuard::spawn();
        let pid_value = child.pid();
        let pidfile = state_root.join(format!("{}.pid", vm.name));
        fs::write(&pidfile, format!("{pid_value}"))?;

        let reaper = thread::spawn(move || {
            loop {
                let mut status = 0;
                let res = unsafe { libc::waitpid(pid_value, &mut status, libc::WNOHANG) };
                if res == 0 {
                    thread::sleep(Duration::from_millis(20));
                    continue;
                }
                break;
            }
        });

        let timeouts = ShutdownTimeouts::new(
            Duration::from_secs(2),
            Duration::from_millis(200),
            Duration::from_millis(200),
        );
        let report = shutdown_vm(&vm, &state_root, timeouts, None)?;
        reaper.join().unwrap();

        assert!(report.changed);
        assert!(report.diagnostics.is_empty());
        assert_eq!(report.outcome, ShutdownOutcome::Forced);
        assert!(!pidfile.exists());

        let mut iter = report.events.iter();

        match iter.next() {
            Some(Event::ShutdownRequested { vm: event_vm }) => assert_eq!(event_vm, &vm.name),
            other => panic!("expected ShutdownRequested first, got {other:?}"),
        }

        match iter.next() {
            Some(Event::CooperativeAttempted {
                vm: event_vm,
                method,
                timeout_ms,
            }) => {
                assert_eq!(event_vm, &vm.name);
                assert_eq!(*method, CooperativeMethod::Unavailable);
                assert_eq!(*timeout_ms, 0);
            }
            other => panic!("expected CooperativeAttempted second, got {other:?}"),
        }

        match iter.next() {
            Some(Event::CooperativeTimedOut {
                vm: event_vm,
                waited_ms,
                reason,
                detail,
            }) => {
                assert_eq!(event_vm, &vm.name);
                assert_eq!(*waited_ms, 0);
                assert_eq!(*reason, CooperativeTimeoutReason::ChannelUnavailable);
                let detail = detail.as_deref().unwrap_or_default();
                assert!(
                    detail.contains(".qmp") || detail.contains("QMP socket"),
                    "expected detail to mention qmp socket, got {detail}"
                );
            }
            other => panic!("expected CooperativeTimedOut third, got {other:?}"),
        }

        match iter.next() {
            Some(Event::ShutdownEscalated {
                vm: event_vm,
                signal,
                timeout_ms,
            }) => {
                assert_eq!(event_vm, &vm.name);
                assert_eq!(*signal, ShutdownSignal::Sigterm);
                assert_eq!(*timeout_ms, Some(200));
            }
            other => panic!("expected ShutdownEscalated fourth, got {other:?}"),
        }

        match iter.next() {
            Some(Event::ShutdownComplete {
                vm: event_vm,
                outcome,
                changed,
                ..
            }) => {
                assert_eq!(event_vm, &vm.name);
                assert_eq!(*outcome, ShutdownOutcome::Forced);
                assert!(*changed);
            }
            other => panic!("expected ShutdownComplete last, got {other:?}"),
        }

        match iter.next() {
            Some(Event::EphemeralLayerDiscarded {
                vm: event_vm,
                reason,
                ..
            }) => {
                assert_eq!(event_vm, &vm.name);
                assert_eq!(*reason, EphemeralCleanupReason::Shutdown);
            }
            other => {
                panic!("expected EphemeralLayerDiscarded after ShutdownComplete, got {other:?}")
            }
        }

        assert!(
            iter.next().is_none(),
            "no additional events expected after shutdown"
        );

        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn cooperative_shutdown_handles_stale_qmp_socket()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        use std::os::unix::net::UnixListener;
        use std::thread;
        use std::time::Duration;

        let temp = tempdir()?;
        let state_root = temp.path().to_path_buf();
        let vm = sample_vm(&state_root);

        let child = ForkGuard::spawn();
        let pid_value = child.pid();
        let pidfile = state_root.join(format!("{}.pid", vm.name));
        fs::write(&pidfile, format!("{pid_value}"))?;

        let socket_path = state_root.join(format!("{}.qmp", vm.name));
        {
            let listener = UnixListener::bind(&socket_path)?;
            drop(listener);
        }
        assert!(
            socket_path.exists(),
            "stale qmp socket should remain on disk"
        );

        let reaper = thread::spawn(move || {
            loop {
                let mut status = 0;
                let res = unsafe { libc::waitpid(pid_value, &mut status, libc::WNOHANG) };
                if res == 0 {
                    thread::sleep(Duration::from_millis(20));
                    continue;
                }
                break;
            }
        });

        let timeouts = ShutdownTimeouts::new(
            Duration::from_millis(200),
            Duration::from_millis(200),
            Duration::from_millis(200),
        );
        let report = shutdown_vm(&vm, &state_root, timeouts, None)?;
        reaper.join().unwrap();

        assert!(report.changed);
        assert_eq!(report.outcome, ShutdownOutcome::Forced);
        assert!(
            !socket_path.exists(),
            "cleanup should remove stale qmp socket"
        );

        let mut iter = report.events.iter();

        match iter.next() {
            Some(Event::ShutdownRequested { vm: event_vm }) => assert_eq!(event_vm, &vm.name),
            other => panic!("expected ShutdownRequested first, got {other:?}"),
        }

        match iter.next() {
            Some(Event::CooperativeAttempted {
                vm: event_vm,
                method,
                timeout_ms,
            }) => {
                assert_eq!(event_vm, &vm.name);
                assert_eq!(*method, CooperativeMethod::Acpi);
                assert_eq!(*timeout_ms, 200);
            }
            other => panic!("expected CooperativeAttempted second, got {other:?}"),
        }

        match iter.next() {
            Some(Event::CooperativeTimedOut {
                vm: event_vm,
                waited_ms,
                reason,
                detail,
            }) => {
                assert_eq!(event_vm, &vm.name);
                assert_eq!(*waited_ms, 0);
                assert_eq!(*reason, CooperativeTimeoutReason::ChannelUnavailable);
                let detail = detail.as_deref().unwrap_or_default();
                assert!(
                    detail.contains("Connection refused")
                        || detail.contains("connection closed unexpectedly"),
                    "expected detail to explain connection failure, got {detail}"
                );
            }
            other => panic!("expected CooperativeTimedOut third, got {other:?}"),
        }

        match iter.next() {
            Some(Event::ShutdownEscalated {
                vm: event_vm,
                signal,
                timeout_ms,
            }) => {
                assert_eq!(event_vm, &vm.name);
                assert_eq!(*signal, ShutdownSignal::Sigterm);
                assert_eq!(*timeout_ms, Some(200));
            }
            other => panic!("expected SIGTERM escalation fourth, got {other:?}"),
        }

        let completion_event = match iter.next() {
            Some(Event::ShutdownEscalated {
                vm: event_vm,
                signal,
                timeout_ms,
            }) => {
                assert_eq!(event_vm, &vm.name);
                assert_eq!(*signal, ShutdownSignal::Sigkill);
                assert_eq!(*timeout_ms, Some(200));
                iter.next()
                    .expect("expected ShutdownComplete event after SIGKILL escalation")
            }
            Some(event @ Event::ShutdownComplete { .. }) => event,
            other => {
                panic!("expected optional SIGKILL escalation or ShutdownComplete, got {other:?}")
            }
        };

        match completion_event {
            Event::ShutdownComplete {
                vm: event_vm,
                outcome,
                changed,
                ..
            } => {
                assert_eq!(event_vm, &vm.name);
                assert_eq!(*outcome, ShutdownOutcome::Forced);
                assert!(*changed);
            }
            other => panic!("unexpected event while finalizing shutdown: {other:?}"),
        }

        match iter.next() {
            Some(Event::EphemeralLayerDiscarded {
                vm: event_vm,
                reason,
                ..
            }) => {
                assert_eq!(event_vm, &vm.name);
                assert_eq!(*reason, EphemeralCleanupReason::Shutdown);
            }
            other => {
                panic!("expected EphemeralLayerDiscarded after ShutdownComplete, got {other:?}")
            }
        }

        assert!(
            iter.next().is_none(),
            "no extra events expected after completion"
        );

        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn cooperative_shutdown_times_out_and_escalates()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        use serde_json::Value;
        use std::io::{BufRead, BufReader, Write};
        use std::os::unix::net::UnixListener;
        use std::thread;
        use std::time::Duration;

        let temp = tempdir()?;
        let state_root = temp.path().to_path_buf();
        let vm = sample_vm(&state_root);

        let child = ForkGuard::spawn();
        let pidfile = state_root.join(format!("{}.pid", vm.name));
        let pid_value = child.pid();
        fs::write(&pidfile, format!("{pid_value}"))?;
        let socket_path = state_root.join(format!("{}.qmp", vm.name));
        let listener = UnixListener::bind(&socket_path)?;

        let reaper = thread::spawn(move || {
            loop {
                let mut status = 0;
                let res = unsafe { libc::waitpid(pid_value, &mut status, libc::WNOHANG) };
                if res == 0 {
                    thread::sleep(Duration::from_millis(20));
                    continue;
                }
                break;
            }
        });

        let qmp_thread = thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let greeting = r#"{"QMP":{"version":{"qemu":{"major":8,"minor":2,"micro":0}},"capabilities":[]}}"#;
                stream.write_all(greeting.as_bytes()).unwrap();
                stream.write_all(b"\n").unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap() == 0 {
                        break;
                    }
                    let value: Value = serde_json::from_str(&line).unwrap();
                    if value.get("execute").is_some() {
                        stream.write_all(br#"{"return":{}}"#).unwrap();
                        stream.write_all(b"\n").unwrap();
                        if value
                            .get("execute")
                            .and_then(|v| v.as_str())
                            .map(|cmd| cmd == "system_powerdown")
                            .unwrap_or(false)
                        {
                            break;
                        }
                    }
                }
            }
        });

        let timeouts = ShutdownTimeouts::new(
            Duration::from_millis(200),
            Duration::from_millis(500),
            Duration::from_millis(500),
        );
        let report = shutdown_vm(&vm, &state_root, timeouts, None)?;
        qmp_thread.join().unwrap();
        reaper.join().unwrap();

        assert!(report.changed);
        assert_eq!(report.outcome, ShutdownOutcome::Forced);
        assert!(!pidfile.exists());

        assert!(
            report.events.iter().any(|event| matches!(
                event,
                Event::ShutdownEscalated {
                    signal: ShutdownSignal::Sigterm,
                    ..
                }
            )),
            "expected SIGTERM escalation event"
        );

        let mut iter = report.events.iter();

        match iter.next() {
            Some(Event::ShutdownRequested { vm: event_vm }) => assert_eq!(event_vm, &vm.name),
            other => panic!("expected ShutdownRequested first, got {other:?}"),
        }

        match iter.next() {
            Some(Event::CooperativeAttempted {
                vm: event_vm,
                method,
                timeout_ms,
            }) => {
                assert_eq!(event_vm, &vm.name);
                assert_eq!(*method, CooperativeMethod::Acpi);
                assert_eq!(*timeout_ms, 200);
            }
            other => panic!("expected CooperativeAttempted second, got {other:?}"),
        }

        match iter.next() {
            Some(Event::CooperativeTimedOut {
                vm: event_vm,
                waited_ms,
                reason,
                detail,
            }) => {
                assert_eq!(event_vm, &vm.name);
                assert_eq!(*waited_ms, 200);
                assert_eq!(*reason, CooperativeTimeoutReason::TimeoutExpired);
                assert!(detail.is_none());
            }
            other => panic!("expected CooperativeTimedOut third, got {other:?}"),
        }

        match iter.next() {
            Some(Event::ShutdownEscalated {
                vm: event_vm,
                signal,
                ..
            }) => {
                assert_eq!(event_vm, &vm.name);
                assert_eq!(*signal, ShutdownSignal::Sigterm);
            }
            other => panic!("expected ShutdownEscalated fourth, got {other:?}"),
        }

        let completion_event = match iter.next() {
            Some(Event::ShutdownEscalated {
                vm: event_vm,
                signal,
                timeout_ms,
            }) => {
                assert_eq!(event_vm, &vm.name);
                assert_eq!(*signal, ShutdownSignal::Sigkill);
                assert_eq!(*timeout_ms, Some(500));
                iter.next()
                    .expect("expected ShutdownComplete event after SIGKILL escalation")
            }
            Some(event @ Event::ShutdownComplete { .. }) => event,
            other => {
                panic!("expected optional SIGKILL escalation or ShutdownComplete, got {other:?}")
            }
        };

        match completion_event {
            Event::ShutdownComplete {
                vm: event_vm,
                outcome,
                changed,
                ..
            } => {
                assert_eq!(event_vm, &vm.name);
                assert_eq!(*outcome, ShutdownOutcome::Forced);
                assert!(*changed);
            }
            other => panic!("expected ShutdownComplete event, got {other:?}"),
        }

        match iter.next() {
            Some(Event::EphemeralLayerDiscarded {
                vm: event_vm,
                reason,
                ..
            }) => {
                assert_eq!(event_vm, &vm.name);
                assert_eq!(*reason, EphemeralCleanupReason::Shutdown);
            }
            other => {
                panic!("expected EphemeralLayerDiscarded after ShutdownComplete, got {other:?}")
            }
        }

        assert!(
            iter.next().is_none(),
            "no extra events expected after completion"
        );

        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn cooperative_shutdown_reports_channel_error_details()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        use std::io::Write;
        use std::os::unix::net::UnixListener;
        use std::thread;
        use std::time::Duration;

        let temp = tempdir()?;
        let state_root = temp.path().to_path_buf();
        let vm = sample_vm(&state_root);

        let child = ForkGuard::spawn();
        let pid_value = child.pid();
        let pidfile = state_root.join(format!("{}.pid", vm.name));
        fs::write(&pidfile, format!("{pid_value}"))?;

        let socket_path = state_root.join(format!("{}.qmp", vm.name));
        let listener = UnixListener::bind(&socket_path)?;

        let reaper = thread::spawn(move || {
            loop {
                let mut status = 0;
                let res = unsafe { libc::waitpid(pid_value, &mut status, libc::WNOHANG) };
                if res == 0 {
                    thread::sleep(Duration::from_millis(20));
                    continue;
                }
                break;
            }
        });

        let qmp_thread = thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let _ = stream.write_all(br#"{"bogus":"not-qmp"}"#);
                let _ = stream.write_all(b"\n");
            }
        });

        let timeouts = ShutdownTimeouts::new(
            Duration::from_millis(200),
            Duration::from_millis(500),
            Duration::from_millis(500),
        );
        let report = shutdown_vm(&vm, &state_root, timeouts, None)?;
        qmp_thread.join().unwrap();
        reaper.join().unwrap();

        assert!(report.changed);
        assert_eq!(report.outcome, ShutdownOutcome::Forced);
        assert!(!pidfile.exists());

        assert_eq!(report.diagnostics.len(), 1);
        let diag = &report.diagnostics[0];
        assert_eq!(diag.severity, Severity::Warning);
        assert!(
            diag.message
                .contains("Failed graceful shutdown for VM `devbox`: Unexpected QMP greeting"),
            "unexpected diagnostic message: {}",
            diag.message
        );
        assert_eq!(
            diag.help.as_deref(),
            Some(
                "Ensure QEMU launched with QMP support or allow Castra to manage the VM lifecycle."
            )
        );

        let mut iter = report.events.iter();

        match iter.next() {
            Some(Event::ShutdownRequested { vm: event_vm }) => assert_eq!(event_vm, &vm.name),
            other => panic!("expected ShutdownRequested first, got {other:?}"),
        }

        match iter.next() {
            Some(Event::CooperativeAttempted {
                vm: event_vm,
                method,
                timeout_ms,
            }) => {
                assert_eq!(event_vm, &vm.name);
                assert_eq!(*method, CooperativeMethod::Acpi);
                assert_eq!(*timeout_ms, 200);
            }
            other => panic!("expected CooperativeAttempted second, got {other:?}"),
        }

        match iter.next() {
            Some(Event::CooperativeTimedOut {
                vm: event_vm,
                waited_ms,
                reason,
                detail,
            }) => {
                assert_eq!(event_vm, &vm.name);
                assert_eq!(
                    *waited_ms, 0,
                    "channel error should conclude without waiting for the cooperative timeout"
                );
                assert_eq!(*reason, CooperativeTimeoutReason::ChannelError);
                let detail_str = detail
                    .as_ref()
                    .expect("detail should be provided for channel errors");
                assert!(
                    detail_str.contains("Unexpected QMP greeting"),
                    "expected QMP greeting detail, got {detail_str}"
                );
            }
            other => panic!("expected CooperativeTimedOut third, got {other:?}"),
        }

        match iter.next() {
            Some(Event::ShutdownEscalated {
                vm: event_vm,
                signal,
                ..
            }) => {
                assert_eq!(event_vm, &vm.name);
                assert_eq!(*signal, ShutdownSignal::Sigterm);
            }
            other => panic!("expected ShutdownEscalated fourth, got {other:?}"),
        }

        match iter.next() {
            Some(Event::ShutdownComplete {
                vm: event_vm,
                outcome,
                changed,
                ..
            }) => {
                assert_eq!(event_vm, &vm.name);
                assert_eq!(*outcome, ShutdownOutcome::Forced);
                assert!(*changed);
            }
            other => panic!("expected ShutdownComplete fifth, got {other:?}"),
        }

        match iter.next() {
            Some(Event::EphemeralLayerDiscarded {
                vm: event_vm,
                reason,
                ..
            }) => {
                assert_eq!(event_vm, &vm.name);
                assert_eq!(*reason, EphemeralCleanupReason::Shutdown);
            }
            other => {
                panic!("expected EphemeralLayerDiscarded after ShutdownComplete, got {other:?}")
            }
        }

        assert!(
            iter.next().is_none(),
            "no additional events expected after shutdown"
        );

        Ok(())
    }
}
