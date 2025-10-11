mod cli;
mod config;
mod error;

use std::cmp;
use std::collections::{HashSet, VecDeque};
use std::ffi::OsStr;
use std::fs;
use std::io::{self, BufRead, BufReader, Seek, SeekFrom, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

use clap::{CommandFactory, Parser, error::ErrorKind};
use libc::{self, pid_t};

use crate::cli::{Cli, Commands, DownArgs, InitArgs, LogsArgs, PortsArgs, StatusArgs, UpArgs};
use crate::config::{PortForward, PortProtocol, ProjectConfig, VmDefinition, load_project_config};
use crate::error::{CliError, CliResult};

fn main() -> ExitCode {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(err) => {
            let _ = err.print();
            return match err.kind() {
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion => ExitCode::SUCCESS,
                _ => ExitCode::from(64),
            };
        }
    };

    let Cli { config, command } = cli;

    let command = match command {
        Some(cmd) => cmd,
        None => {
            let mut command = Cli::command();
            let _ = command.print_help();
            println!();
            return ExitCode::from(64);
        }
    };

    let exit = match command {
        Commands::Init(args) => handle_init(args, config.as_ref()),
        Commands::Up(args) => handle_up(args, config.as_ref()),
        Commands::Down(args) => handle_down(args, config.as_ref()),
        Commands::Status(args) => handle_status(args, config.as_ref()),
        Commands::Ports(args) => handle_ports(args, config.as_ref()),
        Commands::Logs(args) => handle_logs(args, config.as_ref()),
    };

    match exit {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("Error: {err}");
            err.exit_code()
        }
    }
}

fn handle_init(args: InitArgs, config_override: Option<&PathBuf>) -> CliResult<()> {
    let target_path = preferred_config_target(config_override, args.output.as_ref());
    let project_name = args
        .project_name
        .clone()
        .unwrap_or_else(|| default_project_name(&target_path));

    if target_path.exists() && !args.force {
        return Err(CliError::AlreadyInitialized {
            path: target_path.clone(),
        });
    }

    if let Some(parent) = target_path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent).map_err(|source| CliError::CreateDir {
            path: parent.to_path_buf(),
            source,
        })?;
    }

    let workdir = target_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".castra");

    fs::create_dir_all(&workdir).map_err(|source| CliError::CreateDir {
        path: workdir.clone(),
        source,
    })?;

    let config_contents = default_config_contents(&project_name);
    fs::write(&target_path, config_contents).map_err(|source| CliError::WriteConfig {
        path: target_path.clone(),
        source,
    })?;

    println!("✔ Created castra project scaffold.");
    println!("  config  → {}", target_path.display());
    println!("  workdir → {}", workdir.display());
    println!();
    println!("Next steps:");
    println!("  • Update `base_image` in the config to point at your QCOW2 base image.");
    println!("  • Run `castra up` once the image is prepared.");

    Ok(())
}

fn handle_up(args: UpArgs, config_override: Option<&PathBuf>) -> CliResult<()> {
    let config_path = resolve_config_path(config_override, args.skip_discovery)?;
    let project = load_project_config(&config_path)?;

    for warning in &project.warnings {
        eprintln!("Warning: {warning}");
    }

    let (status_rows, mut status_warnings) = collect_vm_status(&project);
    let running: Vec<_> = status_rows
        .iter()
        .filter(|row| row.state == "running")
        .map(|row| row.name.clone())
        .collect();
    for warning in status_warnings.drain(..) {
        eprintln!("Warning: {warning}");
    }
    if !running.is_empty() {
        return Err(CliError::PreflightFailed {
            message: format!(
                "VMs already running: {}. Use `castra status` or `castra down` before invoking `up` again.",
                running.join(", ")
            ),
        });
    }

    let context = prepare_runtime_context(&project)?;
    ensure_ports_available(&project)?;

    for vm in &project.vms {
        ensure_vm_assets(vm, &context)?;
    }

    for vm in &project.vms {
        launch_vm(vm, &context)?;
    }

    println!("Launched {} VM(s).", project.vms.len());
    println!("Use `castra status` to monitor startup progress.");
    Ok(())
}

fn handle_down(args: DownArgs, config_override: Option<&PathBuf>) -> CliResult<()> {
    let config_path = resolve_config_path(config_override, args.skip_discovery)?;
    let project = load_project_config(&config_path)?;

    for warning in &project.warnings {
        eprintln!("Warning: {warning}");
    }

    let state_root = config_state_root(&project);
    let mut had_running = false;

    for vm in &project.vms {
        if shutdown_vm(vm, &state_root)? {
            had_running = true;
        }
    }

    if had_running {
        println!("All VMs have been stopped.");
    } else {
        println!("No running VMs detected.");
    }

    Ok(())
}

fn handle_status(args: StatusArgs, config_override: Option<&PathBuf>) -> CliResult<()> {
    let config_path = resolve_config_path(config_override, args.skip_discovery)?;
    let project = load_project_config(&config_path)?;

    for warning in &project.warnings {
        eprintln!("Warning: {warning}");
    }

    let (status_rows, status_warnings) = collect_vm_status(&project);
    for warning in status_warnings {
        eprintln!("Warning: {warning}");
    }

    print_status_table(&project, &status_rows);
    Ok(())
}

fn handle_ports(args: PortsArgs, config_override: Option<&PathBuf>) -> CliResult<()> {
    let config_path = resolve_config_path(config_override, false)?;
    let project = load_project_config(&config_path)?;

    for warning in &project.warnings {
        eprintln!("Warning: {warning}");
    }

    print_port_overview(&project, args.verbose);
    Ok(())
}

fn print_port_overview(project: &ProjectConfig, verbose: bool) {
    println!(
        "Project: {} ({})",
        project.project_name,
        project.file_path.display()
    );
    println!("Config version: {}", project.version);
    println!("Broker endpoint: 127.0.0.1:{}", project.broker.port);
    println!("(start the broker via `castra up` once available)");
    println!();

    let (conflicts, broker_collision) = project.port_conflicts();
    let conflict_ports: HashSet<u16> = conflicts.iter().map(|c| c.port).collect();
    let broker_conflict_port = broker_collision.as_ref().map(|c| c.port);

    let mut rows = Vec::new();
    for vm in &project.vms {
        for forward in &vm.port_forwards {
            rows.push((
                vm.name.as_str(),
                forward.host,
                forward.guest,
                forward.protocol,
            ));
        }
    }

    let vm_width = cmp::max(
        "VM".len(),
        project
            .vms
            .iter()
            .map(|vm| vm.name.len())
            .max()
            .unwrap_or(0),
    );

    if rows.is_empty() {
        println!(
            "No port forwards declared in {}.",
            project.file_path.display()
        );
    } else {
        println!("Declared forwards:");
        println!(
            "  {vm:<width$}  {:>5}  {:>5}  {:<5}  {}",
            "HOST",
            "GUEST",
            "PROTO",
            "STATUS",
            vm = "VM",
            width = vm_width
        );

        for (vm_name, host, guest, protocol) in rows {
            let mut status = "declared";
            if conflict_ports.contains(&host) {
                status = "conflict";
            } else if broker_conflict_port == Some(host) {
                status = "broker-reserved";
            }

            println!(
                "  {vm:<width$}  {:>5}  {:>5}  {:<5}  {status}",
                host,
                guest,
                protocol,
                vm = vm_name,
                width = vm_width
            );
        }
    }

    let without_forwards: Vec<&str> = project
        .vms
        .iter()
        .filter(|vm| vm.port_forwards.is_empty())
        .map(|vm| vm.name.as_str())
        .collect();

    if !without_forwards.is_empty() {
        println!();
        println!("VMs without host forwards: {}", without_forwards.join(", "));
    }

    if verbose {
        println!();
        println!("VM details:");
        for vm in &project.vms {
            println!("  {}", vm.name);
            if let Some(desc) = &vm.description {
                println!("    description: {desc}");
            }
            println!("    base_image: {}", vm.base_image.display());
            println!("    overlay: {}", vm.overlay.display());
            println!("    cpus: {}", vm.cpus);
            println!("    memory: {}", vm.memory.original());
            if let Some(bytes) = vm.memory.bytes() {
                println!("    memory_bytes: {}", bytes);
            }
            if vm.port_forwards.is_empty() {
                println!("    port_forwards: (none)");
            }
        }
        if !project.workflows.init.is_empty() {
            println!();
            println!("Init workflow steps:");
            for step in &project.workflows.init {
                println!("  - {step}");
            }
        }
    }

    if !conflicts.is_empty() {
        eprintln!();
        for conflict in &conflicts {
            eprintln!(
                "Warning: host port {} is declared by multiple VMs: {}.",
                conflict.port,
                conflict.vm_names.join(", ")
            );
        }
    }

    if let Some(collision) = broker_collision {
        eprintln!(
            "Warning: host port {} overlaps with the castra broker. Adjust the broker port or the forward.",
            collision.port
        );
    }
}

fn config_root(project: &ProjectConfig) -> PathBuf {
    project
        .file_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn config_state_root(project: &ProjectConfig) -> PathBuf {
    config_root(project).join(".castra")
}

struct RuntimeContext {
    state_root: PathBuf,
    log_root: PathBuf,
    qemu_system: PathBuf,
    qemu_img: Option<PathBuf>,
}

struct LogSource {
    name: String,
    path: PathBuf,
    offset: u64,
}

fn prepare_runtime_context(project: &ProjectConfig) -> CliResult<RuntimeContext> {
    let state_root = config_state_root(project);
    fs::create_dir_all(&state_root).map_err(|err| CliError::PreflightFailed {
        message: format!(
            "Failed to create castra state directory at {}: {err}",
            state_root.display()
        ),
    })?;

    let log_root = state_root.join("logs");
    fs::create_dir_all(&log_root).map_err(|err| CliError::PreflightFailed {
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
        CliError::PreflightFailed {
            message: "qemu-system binary not found in PATH. Install QEMU (e.g. `brew install qemu` on macOS or `sudo apt install qemu-system` on Debian/Ubuntu).".to_string(),
        }
    })?;

    let qemu_img = find_executable(&["qemu-img", "qemu-img.exe"]);

    Ok(RuntimeContext {
        state_root,
        log_root,
        qemu_system,
        qemu_img,
    })
}

fn ensure_ports_available(project: &ProjectConfig) -> CliResult<()> {
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
        return Err(CliError::PreflightFailed {
            message: format!("Host port conflicts detected:\n{}", lines.join("\n")),
        });
    }

    if let Some(collision) = broker_collision {
        return Err(CliError::PreflightFailed {
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

fn ensure_port_is_free(port: u16, description: &str) -> CliResult<()> {
    let bind_addr = format!("127.0.0.1:{port}");
    match TcpListener::bind(&bind_addr) {
        Ok(listener) => {
            drop(listener);
            Ok(())
        }
        Err(err) if err.kind() == io::ErrorKind::AddrInUse => Err(CliError::PreflightFailed {
            message: format!(
                "Host port {port} ({description}) is already in use. Stop the conflicting service or change the port in castra.toml."
            ),
        }),
        Err(err) => Err(CliError::PreflightFailed {
            message: format!("Unable to check host port {port} for {description}: {err}"),
        }),
    }
}

fn ensure_vm_assets(vm: &VmDefinition, context: &RuntimeContext) -> CliResult<()> {
    if !vm.base_image.is_file() {
        return Err(CliError::PreflightFailed {
            message: format!(
                "Base image for VM `{}` not found at {}. Update `base_image` or make sure the file exists.",
                vm.name,
                vm.base_image.display()
            ),
        });
    }

    if let Some(parent) = vm.overlay.parent() {
        fs::create_dir_all(parent).map_err(|err| CliError::PreflightFailed {
            message: format!(
                "Failed to prepare overlay directory {} for VM `{}`: {err}.",
                parent.display(),
                vm.name
            ),
        })?;
    }

    if !vm.overlay.exists() {
        let Some(qemu_img) = &context.qemu_img else {
            return Err(CliError::PreflightFailed {
                message: format!(
                    "Overlay image for VM `{}` missing at {} and `qemu-img` was not found. Create it manually using:\n  qemu-img create -f qcow2 -b {} {}",
                    vm.name,
                    vm.overlay.display(),
                    vm.base_image.display(),
                    vm.overlay.display()
                ),
            });
        };

        create_overlay(qemu_img, &vm.base_image, &vm.overlay, &vm.name)?;
        println!(
            "Prepared overlay for VM `{}` at {}.",
            vm.name,
            vm.overlay.display()
        );
    }

    Ok(())
}

fn create_overlay(qemu_img: &Path, base: &Path, overlay: &Path, vm_name: &str) -> CliResult<()> {
    let status = Command::new(qemu_img)
        .arg("create")
        .arg("-f")
        .arg("qcow2")
        .arg("-b")
        .arg(base)
        .arg(overlay)
        .status()
        .map_err(|err| CliError::PreflightFailed {
            message: format!(
                "Failed to invoke `{}` while creating overlay for VM `{vm_name}`: {err}",
                qemu_img.display()
            ),
        })?;

    if !status.success() {
        return Err(CliError::PreflightFailed {
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

fn launch_vm(vm: &VmDefinition, context: &RuntimeContext) -> CliResult<()> {
    let pidfile = context.state_root.join(format!("{}.pid", vm.name));
    if pidfile.exists() {
        let _ = fs::remove_file(&pidfile);
    }

    let log_path = context.log_root.join(format!("{}.log", vm.name));
    let log_file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(|err| CliError::LaunchFailed {
            vm: vm.name.clone(),
            message: format!("Could not open log file {}: {err}", log_path.display()),
        })?;
    let log_clone = log_file.try_clone().map_err(|err| CliError::LaunchFailed {
        vm: vm.name.clone(),
        message: format!(
            "Could not duplicate log handle for {}: {err}",
            log_path.display()
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
        .arg("none")
        .stdout(Stdio::from(log_file))
        .stderr(Stdio::from(log_clone));

    if cfg!(target_os = "macos") {
        command.arg("-machine").arg("accel=hvf");
    } else if cfg!(target_os = "linux") {
        command.arg("-enable-kvm");
    }

    command.arg("-cpu").arg("host");

    let status = command.status().map_err(|err| CliError::LaunchFailed {
        vm: vm.name.clone(),
        message: format!("Failed to spawn {}: {err}", context.qemu_system.display()),
    })?;

    if !status.success() {
        return Err(CliError::LaunchFailed {
            vm: vm.name.clone(),
            message: format!(
                "{} exited with status {}.",
                context.qemu_system.display(),
                status.code().unwrap_or(-1)
            ),
        });
    }

    wait_for_pidfile(&pidfile, Duration::from_secs(5)).map_err(|err| CliError::LaunchFailed {
        vm: vm.name.clone(),
        message: format!(
            "QEMU did not write pidfile {} within timeout: {err}",
            pidfile.display()
        ),
    })?;

    println!("→ {}: launched (pidfile {}).", vm.name, pidfile.display());
    Ok(())
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

fn read_tail_lines(path: &Path, limit: usize) -> io::Result<Vec<String>> {
    if limit == 0 {
        return Ok(Vec::new());
    }

    let file = fs::File::open(path)?;
    let reader = BufReader::new(file);
    let mut ring: VecDeque<String> = VecDeque::with_capacity(limit);

    for line in reader.lines() {
        let line = line?;
        if ring.len() == limit {
            ring.pop_front();
        }
        ring.push_back(line);
    }

    Ok(ring.into_iter().collect())
}

fn follow_logs(sources: &mut [LogSource]) -> CliResult<()> {
    println!("--- Following logs (press Ctrl-C to stop) ---");
    loop {
        let mut activity = false;
        for source in sources.iter_mut() {
            match fs::File::open(&source.path) {
                Ok(mut file) => {
                    if source.offset > 0 {
                        if let Err(err) = file.seek(SeekFrom::Start(source.offset)) {
                            return Err(CliError::LogReadFailed {
                                path: source.path.clone(),
                                source: err,
                            });
                        }
                    }

                    let mut reader = BufReader::new(file);
                    let mut buffer = String::new();
                    loop {
                        buffer.clear();
                        let bytes = reader.read_line(&mut buffer).map_err(|err| {
                            CliError::LogReadFailed {
                                path: source.path.clone(),
                                source: err,
                            }
                        })?;
                        if bytes == 0 {
                            break;
                        }
                        source.offset += bytes as u64;
                        while buffer.ends_with('\n') || buffer.ends_with('\r') {
                            buffer.pop();
                        }
                        if buffer.is_empty() {
                            println!("{} |", source.name);
                        } else {
                            println!("{} | {}", source.name, buffer);
                        }
                        activity = true;
                    }
                }
                Err(err) if err.kind() == io::ErrorKind::NotFound => {
                    continue;
                }
                Err(err) => {
                    return Err(CliError::LogReadFailed {
                        path: source.path.clone(),
                        source: err,
                    });
                }
            }
        }

        io::stdout().flush().ok();

        if !activity {
            thread::sleep(Duration::from_millis(500));
        }
    }
}

fn shutdown_vm(vm: &VmDefinition, state_root: &Path) -> CliResult<bool> {
    let pidfile = state_root.join(format!("{}.pid", vm.name));
    if !pidfile.is_file() {
        println!("→ {}: already stopped.", vm.name);
        return Ok(false);
    }

    let contents = fs::read_to_string(&pidfile).map_err(|err| CliError::ShutdownFailed {
        vm: vm.name.clone(),
        message: format!("Unable to read pidfile {}: {err}", pidfile.display()),
    })?;

    let trimmed = contents.trim();
    let pid: pid_t = trimmed.parse().map_err(|_| CliError::ShutdownFailed {
        vm: vm.name.clone(),
        message: format!(
            "Pidfile {} contained invalid pid `{trimmed}`.",
            pidfile.display()
        ),
    })?;

    let term = unsafe { libc::kill(pid, libc::SIGTERM) };
    if term != 0 {
        let errno = io::Error::last_os_error()
            .raw_os_error()
            .unwrap_or_default();
        if errno == libc::ESRCH {
            println!(
                "→ {}: removing stale pidfile (process {pid} already exited).",
                vm.name
            );
            let _ = fs::remove_file(&pidfile);
            return Ok(false);
        }

        return Err(CliError::ShutdownFailed {
            vm: vm.name.clone(),
            message: format!("Failed to send SIGTERM to pid {pid}: errno {errno}"),
        });
    }

    println!("→ {}: sent SIGTERM to pid {}.", vm.name, pid);
    if !wait_for_process_exit(pid, Duration::from_secs(10)).map_err(|err| {
        CliError::ShutdownFailed {
            vm: vm.name.clone(),
            message: format!("Error while waiting for pid {pid} to exit: {err}"),
        }
    })? {
        println!("→ {}: escalating to SIGKILL (pid {}).", vm.name, pid);
        let kill_res = unsafe { libc::kill(pid, libc::SIGKILL) };
        if kill_res != 0 {
            let errno = io::Error::last_os_error()
                .raw_os_error()
                .unwrap_or_default();
            if errno != libc::ESRCH {
                return Err(CliError::ShutdownFailed {
                    vm: vm.name.clone(),
                    message: format!("Failed to send SIGKILL to pid {pid}: errno {errno}"),
                });
            }
        }

        if !wait_for_process_exit(pid, Duration::from_secs(5)).map_err(|err| {
            CliError::ShutdownFailed {
                vm: vm.name.clone(),
                message: format!("Error while waiting for pid {pid} after SIGKILL: {err}"),
            }
        })? {
            return Err(CliError::ShutdownFailed {
                vm: vm.name.clone(),
                message: format!("Process {pid} did not exit after SIGKILL."),
            });
        }
    }

    if let Err(err) = fs::remove_file(&pidfile) {
        return Err(CliError::ShutdownFailed {
            vm: vm.name.clone(),
            message: format!(
                "VM stopped but failed to remove pidfile {}: {err}",
                pidfile.display()
            ),
        });
    }

    println!("→ {}: stopped.", vm.name);
    Ok(true)
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

#[derive(Debug)]
struct VmStatusRow {
    name: String,
    state: String,
    cpus: u32,
    memory: String,
    uptime: Option<Duration>,
    forwards: String,
}

fn collect_vm_status(project: &ProjectConfig) -> (Vec<VmStatusRow>, Vec<String>) {
    let mut rows = Vec::with_capacity(project.vms.len());
    let mut warnings = Vec::new();
    let state_root = config_state_root(project);

    for vm in &project.vms {
        let pidfile = state_root.join(format!("{}.pid", vm.name));
        let (state, uptime, mut state_warnings) = inspect_vm_state(&pidfile, &vm.name);
        warnings.append(&mut state_warnings);

        rows.push(VmStatusRow {
            name: vm.name.clone(),
            state,
            cpus: vm.cpus,
            memory: vm.memory.original().replace(' ', ""),
            uptime,
            forwards: format_port_forwards(&vm.port_forwards),
        });
    }

    (rows, warnings)
}

fn inspect_vm_state(pidfile: &Path, vm_name: &str) -> (String, Option<Duration>, Vec<String>) {
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
        "Unable to determine if VM `{vm_name}` is running (pid {pid}, errno {errno}).",
        pid = pid,
        errno = errno
    ));
    ("unknown".to_string(), None, warnings)
}

fn uptime_from_pidfile(pidfile: &Path) -> Option<Duration> {
    let metadata = fs::metadata(pidfile).ok()?;
    let modified = metadata.modified().ok()?;
    SystemTime::now().duration_since(modified).ok()
}

fn format_port_forwards(forwards: &[PortForward]) -> String {
    if forwards.is_empty() {
        return "—".to_string();
    }

    let mut entries = Vec::with_capacity(forwards.len());
    for forward in forwards {
        entries.push(format!(
            "{}->{}{}",
            forward.host,
            forward.guest,
            match forward.protocol {
                PortProtocol::Tcp => "/tcp",
                PortProtocol::Udp => "/udp",
            }
        ));
    }
    entries.join(", ")
}

fn format_uptime(value: Option<Duration>) -> String {
    if let Some(duration) = value {
        let total_seconds = duration.as_secs();
        let hours = total_seconds / 3600;
        let minutes = (total_seconds % 3600) / 60;
        let seconds = total_seconds % 60;
        return format!("{hours:02}:{minutes:02}:{seconds:02}");
    }
    "—".to_string()
}

fn print_status_table(project: &ProjectConfig, rows: &[VmStatusRow]) {
    println!(
        "Project: {} ({})",
        project.project_name,
        project.file_path.display()
    );
    println!("Config version: {}", project.version);
    println!("Broker endpoint: 127.0.0.1:{}", project.broker.port);
    println!();

    if rows.is_empty() {
        println!("No VMs defined in configuration.");
        return;
    }

    let vm_width = rows
        .iter()
        .map(|row| row.name.len())
        .max()
        .unwrap_or(2)
        .max("VM".len());
    let state_width = rows
        .iter()
        .map(|row| row.state.len())
        .max()
        .unwrap_or(5)
        .max("STATE".len());
    let cpu_width = rows
        .iter()
        .map(|row| row.cpus.to_string().len())
        .max()
        .unwrap_or(3)
        .max("CPU".len());
    let mem_width = rows
        .iter()
        .map(|row| row.memory.len())
        .max()
        .unwrap_or(3)
        .max("MEM".len());
    let uptime_width = rows
        .iter()
        .map(|row| format_uptime(row.uptime).len())
        .max()
        .unwrap_or(1)
        .max("UPTIME".len());

    println!(
        "{:<vm_width$}  {:<state_width$}  {:>cpu_width$}  {:>mem_width$}  {:>uptime_width$}  {}",
        "VM",
        "STATE",
        "CPU",
        "MEM",
        "UPTIME",
        "FORWARDS",
        vm_width = vm_width,
        state_width = state_width,
        cpu_width = cpu_width,
        mem_width = mem_width,
        uptime_width = uptime_width
    );

    for row in rows {
        println!(
            "{:<vm_width$}  {:<state_width$}  {:>cpu_width$}  {:>mem_width$}  {:>uptime_width$}  {}",
            row.name,
            row.state,
            row.cpus,
            row.memory,
            format_uptime(row.uptime),
            row.forwards,
            vm_width = vm_width,
            state_width = state_width,
            cpu_width = cpu_width,
            mem_width = mem_width,
            uptime_width = uptime_width
        );
    }
}

fn handle_logs(args: LogsArgs, config_override: Option<&PathBuf>) -> CliResult<()> {
    let config_path = resolve_config_path(config_override, false)?;
    let project = load_project_config(&config_path)?;

    for warning in &project.warnings {
        eprintln!("Warning: {warning}");
    }

    let log_dir = config_state_root(&project).join("logs");
    let mut sources = Vec::new();

    for vm in &project.vms {
        let log_path = log_dir.join(format!("{}.log", vm.name));
        println!("== {} ({}) ==", vm.name, log_path.display());

        match read_tail_lines(&log_path, args.tail) {
            Ok(lines) if lines.is_empty() => println!("{} | (no log entries yet)", vm.name),
            Ok(lines) => {
                for line in lines {
                    if line.is_empty() {
                        println!("{} |", vm.name);
                    } else {
                        println!("{} | {}", vm.name, line);
                    }
                }
            }
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                println!("{} | (log file not created yet)", vm.name);
            }
            Err(err) => {
                return Err(CliError::LogReadFailed {
                    path: log_path.clone(),
                    source: err,
                });
            }
        }

        let offset = fs::metadata(&log_path).map(|meta| meta.len()).unwrap_or(0);
        sources.push(LogSource {
            name: vm.name.clone(),
            path: log_path,
            offset,
        });

        println!();
    }

    if args.follow {
        follow_logs(&mut sources)?;
    }

    Ok(())
}

fn preferred_config_target(
    config_override: Option<&PathBuf>,
    output_flag: Option<&PathBuf>,
) -> PathBuf {
    if let Some(path) = output_flag {
        return path.clone();
    }
    if let Some(path) = config_override {
        return path.clone();
    }
    PathBuf::from("castra.toml")
}

fn default_project_name(target_path: &Path) -> String {
    if let Some(parent) = target_path.parent().filter(|p| !p.as_os_str().is_empty()) {
        if let Some(name) = parent.file_name().and_then(OsStr::to_str) {
            if !name.is_empty() {
                return name.to_string();
            }
        }
    }

    std::env::current_dir()
        .ok()
        .and_then(|path| {
            path.file_name()
                .and_then(OsStr::to_str)
                .map(ToString::to_string)
        })
        .unwrap_or_else(|| "castra-project".to_string())
}

fn default_config_contents(project_name: &str) -> String {
    format!(
        r#"# Castra project configuration
# Visit todo_project_config_and_discovery.md for the roadmap.
version = "0.1.0"

[project]
name = "{project_name}"

[[vms]]
name = "devbox"
description = "Primary development VM"
base_image = "images/devbox-base.qcow2"
overlay = ".castra/devbox-overlay.qcow2"
cpus = 2
memory = "2048 MiB"

  [[vms.port_forwards]]
  host = 2222
  guest = 22
  protocol = "tcp"

  [[vms.port_forwards]]
  host = 8080
  guest = 80
  protocol = "tcp"

[workflows]
init = ["qemu-img create -f qcow2 -b {{{{ base_image }}}} {{{{ overlay }}}}"]
"#
    )
}

fn resolve_config_path(
    config_override: Option<&PathBuf>,
    skip_discovery: bool,
) -> CliResult<PathBuf> {
    if let Some(path) = config_override {
        if path.is_file() {
            return Ok(path.clone());
        } else {
            return Err(CliError::ExplicitConfigMissing { path: path.clone() });
        }
    }

    if skip_discovery {
        let cwd = current_dir()?;
        return Err(CliError::ConfigDiscoveryFailed { search_root: cwd });
    }

    let cwd = current_dir()?;
    discover_config(&cwd).ok_or_else(|| CliError::ConfigDiscoveryFailed { search_root: cwd })
}

fn current_dir() -> CliResult<PathBuf> {
    std::env::current_dir().map_err(|source| CliError::WorkingDirectoryUnavailable { source })
}

fn discover_config(start: &Path) -> Option<PathBuf> {
    let mut cursor = Some(start.to_path_buf());
    while let Some(dir) = cursor {
        let candidate = dir.join("castra.toml");
        if candidate.is_file() {
            return Some(candidate);
        }
        cursor = dir.parent().map(Path::to_path_buf);
    }
    None
}
