use std::cmp;
use std::collections::HashSet;
use std::fs;
use std::io::{self, ErrorKind};
#[cfg(unix)]
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
#[cfg(unix)]
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

use libc::{self, pid_t};
use sysinfo::{Disks, System};

use crate::config::{
    BaseImageSource, ManagedDiskKind, PortForward, PortProtocol, ProjectConfig, VmDefinition,
};
use crate::error::{Error, Result};
use crate::managed::{
    ImageManager, ManagedArtifactEvent, ManagedArtifactKind, ManagedImagePaths, ManagedImageSpec,
    ManagedImageVerification, lookup_managed_image,
};
use serde_json::{Value, json};

use super::diagnostics::{Diagnostic, Severity};
use super::events::{
    CooperativeMethod, CooperativeTimeoutReason, Event, ShutdownOutcome, ShutdownSignal,
};

pub struct RuntimeContext {
    pub state_root: PathBuf,
    pub log_root: PathBuf,
    pub qemu_system: PathBuf,
    pub qemu_img: Option<PathBuf>,
    pub image_manager: ImageManager,
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

#[derive(Debug)]
pub struct AssetPreparation {
    pub assets: ResolvedVmAssets,
    pub managed: Option<ManagedAcquisition>,
    pub overlay_created: bool,
}

#[derive(Debug)]
pub struct ManagedAcquisition {
    pub spec: &'static ManagedImageSpec,
    pub events: Vec<ManagedArtifactEvent>,
    pub verification: ManagedImageVerification,
    pub paths: ManagedImagePaths,
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
            "Failed to create managed image storage directory at {}: {err}",
            image_storage_root.display()
        ),
    })?;

    let image_manager = ImageManager::new(image_storage_root, log_root.clone(), qemu_img.clone());
    let accelerators = detect_available_accelerators(&qemu_system);

    Ok(RuntimeContext {
        state_root,
        log_root,
        qemu_system,
        qemu_img,
        image_manager,
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
    match &vm.base_image {
        BaseImageSource::Path(path) => {
            if !path.is_file() {
                return Err(Error::PreflightFailed {
                    message: format!(
                        "Base image for VM `{}` not found at {}. Update `base_image` or make sure the file exists.",
                        vm.name,
                        path.display()
                    ),
                });
            }

            let overlay_created = ensure_overlay(vm, context, path)?;

            Ok(AssetPreparation {
                assets: ResolvedVmAssets { boot: None },
                managed: None,
                overlay_created,
            })
        }
        BaseImageSource::Managed(reference) => {
            let spec =
                lookup_managed_image(&reference.name, &reference.version).ok_or_else(|| {
                    Error::PreflightFailed {
                        message: format!(
                            "Managed image `{}` version `{}` is not available.",
                            reference.name, reference.version
                        ),
                    }
                })?;

            let outcome = context.image_manager.ensure_image(spec)?;
            let base_disk = match reference.disk {
                ManagedDiskKind::RootDisk => outcome.paths.root_disk.clone(),
            };

            if !base_disk.is_file() {
                return Err(Error::PreflightFailed {
                    message: format!(
                        "Managed base image for VM `{}` missing at {} after acquisition.",
                        vm.name,
                        base_disk.display()
                    ),
                });
            }

            let overlay_created = ensure_overlay(vm, context, &base_disk)?;
            let boot = build_boot_overrides(spec, &outcome.paths)?;

            Ok(AssetPreparation {
                assets: ResolvedVmAssets { boot },
                managed: Some(ManagedAcquisition {
                    spec,
                    events: outcome.events,
                    verification: outcome.verification,
                    paths: outcome.paths,
                }),
                overlay_created,
            })
        }
    }
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

pub fn shutdown_vm(
    vm: &VmDefinition,
    state_root: &Path,
    timeouts: ShutdownTimeouts,
    events: &mut Vec<Event>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<(bool, ShutdownOutcome)> {
    let shutdown_started = Instant::now();
    events.push(Event::ShutdownRequested {
        vm: vm.name.clone(),
    });

    let pidfile = state_root.join(format!("{}.pid", vm.name));
    if !pidfile.is_file() {
        cleanup_qmp_socket(state_root, &vm.name);
        let total_ms = duration_to_millis(shutdown_started.elapsed());
        events.push(Event::ShutdownComplete {
            vm: vm.name.clone(),
            outcome: ShutdownOutcome::Graceful,
            total_ms,
            changed: false,
        });
        return Ok((false, ShutdownOutcome::Graceful));
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
        events.push(Event::ShutdownComplete {
            vm: vm.name.clone(),
            outcome: ShutdownOutcome::Graceful,
            total_ms,
            changed: false,
        });
        return Ok((false, ShutdownOutcome::Graceful));
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

    let graceful_attempt = attempt_graceful_shutdown(state_root, &vm.name);
    let cooperative_method = match graceful_attempt {
        GracefulTrigger::Initiated | GracefulTrigger::Failed { .. } => CooperativeMethod::Acpi,
        GracefulTrigger::Unavailable { .. } => CooperativeMethod::Unavailable,
    };

    events.push(Event::CooperativeAttempted {
        vm: vm.name.clone(),
        method: cooperative_method,
        timeout_ms: graceful_wait_ms,
    });

    match graceful_attempt {
        GracefulTrigger::Initiated => {
            let wait_started = Instant::now();
            if wait_for_process_exit(pid, graceful_wait).map_err(|err| Error::ShutdownFailed {
                vm: vm.name.clone(),
                message: format!(
                    "Error while waiting for pid {pid} during graceful shutdown: {err}"
                ),
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
                events.push(Event::CooperativeSucceeded {
                    vm: vm.name.clone(),
                    elapsed_ms,
                });
                let total_ms = duration_to_millis(shutdown_started.elapsed());
                events.push(Event::ShutdownComplete {
                    vm: vm.name.clone(),
                    outcome: ShutdownOutcome::Graceful,
                    total_ms,
                    changed: true,
                });
                return Ok((true, ShutdownOutcome::Graceful));
            }
            events.push(Event::CooperativeTimedOut {
                vm: vm.name.clone(),
                waited_ms: graceful_wait_ms,
                reason: CooperativeTimeoutReason::TimeoutExpired,
                detail: None,
            });
        }
        GracefulTrigger::Unavailable { detail } => {
            events.push(Event::CooperativeTimedOut {
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
            events.push(Event::CooperativeTimedOut {
                vm: vm.name.clone(),
                waited_ms: 0,
                reason: CooperativeTimeoutReason::ChannelError,
                detail: Some(detail),
            });
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
            events.push(Event::ShutdownComplete {
                vm: vm.name.clone(),
                outcome: ShutdownOutcome::Graceful,
                total_ms,
                changed: false,
            });
            return Ok((false, ShutdownOutcome::Graceful));
        }

        return Err(Error::ShutdownFailed {
            vm: vm.name.clone(),
            message: format!("Failed to send SIGTERM to pid {pid}: errno {errno}"),
        });
    }

    events.push(Event::ShutdownEscalated {
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
        events.push(Event::ShutdownEscalated {
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
    events.push(Event::ShutdownComplete {
        vm: vm.name.clone(),
        outcome,
        total_ms,
        changed: true,
    });
    Ok((true, outcome))
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
            Err(GracefulShutdownError::Unavailable) => GracefulTrigger::Unavailable {
                detail: format!(
                    "QMP socket {} not available for `{vm_name}`",
                    socket.display(),
                    vm_name = vm_name
                ),
            },
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
    Unavailable,
    Io(io::Error),
    Protocol(String),
}

fn send_qmp_powerdown(socket: &Path) -> std::result::Result<(), GracefulShutdownError> {
    if !socket.exists() {
        return Err(GracefulShutdownError::Unavailable);
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
        | io::ErrorKind::PermissionDenied => GracefulShutdownError::Unavailable,
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
        return Err(GracefulShutdownError::Protocol(
            "QMP connection closed unexpectedly.".to_string(),
        ));
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

fn ensure_overlay(vm: &VmDefinition, context: &RuntimeContext, base_disk: &Path) -> Result<bool> {
    if let Some(parent) = vm.overlay.parent() {
        fs::create_dir_all(parent).map_err(|err| Error::PreflightFailed {
            message: format!(
                "Failed to prepare overlay directory {} for VM `{}`: {err}.",
                parent.display(),
                vm.name
            ),
        })?;
    }

    if vm.overlay.exists() {
        return Ok(false);
    }

    let Some(qemu_img) = &context.qemu_img else {
        return Err(Error::PreflightFailed {
            message: format!(
                "Overlay image for VM `{}` missing at {} and `qemu-img` was not found. Create it manually using:\n  qemu-img create -f qcow2 -b {} {}",
                vm.name,
                vm.overlay.display(),
                base_disk.display(),
                vm.overlay.display()
            ),
        });
    };

    let base_format = detect_image_format(qemu_img, base_disk);
    create_overlay(
        qemu_img,
        base_disk,
        &vm.overlay,
        &vm.name,
        base_format.as_deref(),
    )?;
    Ok(true)
}

fn build_boot_overrides(
    spec: &'static ManagedImageSpec,
    paths: &ManagedImagePaths,
) -> Result<Option<BootOverrides>> {
    let kernel_path =
        match spec.qemu.kernel {
            Some(kind) => Some(resolve_managed_artifact(kind, paths).ok_or_else(|| {
                Error::PreflightFailed {
                    message: format!(
                        "Managed image {} requires kernel artifact `{}` but it was not produced.",
                        spec_identifier(spec),
                        kind.describe()
                    ),
                }
            })?),
            None => None,
        };

    let initrd_path =
        match spec.qemu.initrd {
            Some(kind) => Some(resolve_managed_artifact(kind, paths).ok_or_else(|| {
                Error::PreflightFailed {
                    message: format!(
                        "Managed image {} requires initrd artifact `{}` but it was not produced.",
                        spec_identifier(spec),
                        kind.describe()
                    ),
                }
            })?),
            None => None,
        };

    let append = spec.qemu.append.to_string();
    let extra_args: Vec<String> = spec
        .qemu
        .extra_args
        .iter()
        .map(|arg| arg.to_string())
        .collect();
    let machine = spec.qemu.machine.map(|value| value.to_string());

    if kernel_path.is_none()
        && initrd_path.is_none()
        && append.is_empty()
        && extra_args.is_empty()
        && machine.is_none()
    {
        return Ok(None);
    }

    let kernel = kernel_path.ok_or_else(|| Error::PreflightFailed {
        message: format!(
            "Managed image {} provides boot directives but no kernel artifact was resolved.",
            spec_identifier(spec)
        ),
    })?;

    Ok(Some(BootOverrides {
        kernel,
        initrd: initrd_path,
        append,
        extra_args,
        machine,
    }))
}

fn resolve_managed_artifact(
    kind: ManagedArtifactKind,
    paths: &ManagedImagePaths,
) -> Option<PathBuf> {
    match kind {
        ManagedArtifactKind::RootDisk => Some(paths.root_disk.clone()),
        ManagedArtifactKind::Kernel => paths.kernel.clone(),
        ManagedArtifactKind::Initrd => paths.initrd.clone(),
    }
}

fn spec_identifier(spec: &ManagedImageSpec) -> String {
    format!("{}@{}", spec.id, spec.version)
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
    use crate::error::Error;
    use std::fs;
    use std::net::TcpListener;
    use tempfile::tempdir;

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
}
