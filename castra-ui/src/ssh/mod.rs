use std::{
    collections::HashMap,
    io::{BufRead, BufReader, Read, Write},
    process::{Child, ChildStdin, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

use async_channel::Sender;
use castra::core::events::BootstrapPlanSsh;

pub const HANDSHAKE_BANNER: &str = "__castra ssh bridge ready__";
const HANDSHAKE_COMMAND: &str = "printf '__castra ssh bridge ready__\\n'";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SshStream {
    Stdout,
    Stderr,
}

#[derive(Debug, Clone)]
pub enum SshEvent {
    Connecting {
        vm: String,
        command: String,
    },
    Connected {
        vm: String,
    },
    Output {
        vm: String,
        stream: SshStream,
        line: String,
    },
    ConnectionFailed {
        vm: String,
        error: String,
    },
    Disconnected {
        vm: String,
        exit_status: Option<i32>,
    },
}

pub struct SshManager {
    sender: Sender<SshEvent>,
    plans: HashMap<String, BootstrapPlanSsh>,
    connections: HashMap<String, SshConnection>,
}

impl SshManager {
    pub fn new(sender: Sender<SshEvent>) -> Self {
        Self {
            sender,
            plans: HashMap::new(),
            connections: HashMap::new(),
        }
    }

    pub fn register_plan(&mut self, vm: &str, ssh: &BootstrapPlanSsh) {
        self.plans.insert(vm.to_string(), ssh.clone());
    }

    pub fn ensure_connection(&mut self, vm: &str) -> Result<(), String> {
        if self.connections.contains_key(vm) {
            return Ok(());
        }

        let plan = match self.plans.get(vm) {
            Some(plan) => plan.clone(),
            None => {
                let message = format!("No SSH plan available for VM `{vm}`.");
                let _ = self.sender.try_send(SshEvent::ConnectionFailed {
                    vm: vm.to_string(),
                    error: message.clone(),
                });
                return Err(message);
            }
        };

        let command_preview = plan.command();
        let _ = self.sender.try_send(SshEvent::Connecting {
            vm: vm.to_string(),
            command: command_preview,
        });

        match SshConnection::spawn(vm.to_string(), plan, self.sender.clone()) {
            Ok(connection) => {
                let vm_key = vm.to_string();
                self.connections.insert(vm_key.clone(), connection);
                let _ = self
                    .sender
                    .try_send(SshEvent::Connected { vm: vm_key.clone() });
                if let Err(err) = self.send_line(&vm_key, HANDSHAKE_COMMAND) {
                    let _ = self.sender.try_send(SshEvent::ConnectionFailed {
                        vm: vm_key,
                        error: format!("Failed to send SSH handshake: {err}"),
                    });
                }
                Ok(())
            }
            Err(err) => {
                let _ = self.sender.try_send(SshEvent::ConnectionFailed {
                    vm: vm.to_string(),
                    error: err.clone(),
                });
                Err(err)
            }
        }
    }

    pub fn send_line(&mut self, vm: &str, line: &str) -> Result<(), String> {
        let connection = self
            .connections
            .get_mut(vm)
            .ok_or_else(|| format!("No SSH connection for `{vm}`."))?;
        connection.write_line(line)
    }

    pub fn reset(&mut self) {
        for (vm, mut connection) in self.connections.drain() {
            connection.shutdown(&self.sender, &vm);
        }
        self.plans.clear();
    }
}

impl Drop for SshManager {
    fn drop(&mut self) {
        self.reset();
    }
}

struct SshConnection {
    vm: String,
    child: Arc<Mutex<Child>>,
    stdin: Option<ChildStdin>,
    stdout_handle: Option<thread::JoinHandle<()>>,
    stderr_handle: Option<thread::JoinHandle<()>>,
    monitor_handle: Option<thread::JoinHandle<()>>,
    disconnected: Arc<AtomicBool>,
}

impl SshConnection {
    fn spawn(vm: String, plan: BootstrapPlanSsh, sender: Sender<SshEvent>) -> Result<Self, String> {
        let mut command = std::process::Command::new("ssh");
        command.stdin(Stdio::piped());
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());

        if let Some(identity) = plan.identity.as_ref() {
            command.arg("-i");
            command.arg(identity);
        }

        for option in &plan.options {
            command.arg("-o");
            command.arg(option);
        }

        command.arg("-p");
        command.arg(plan.port.to_string());
        command.arg(format!("{}@{}", plan.user, plan.host));

        let mut child = command
            .spawn()
            .map_err(|err| format!("Failed to spawn ssh for `{vm}`: {err}"))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| format!("Failed to capture stdin for ssh `{vm}`"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| format!("Failed to capture stdout for ssh `{vm}`"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| format!("Failed to capture stderr for ssh `{vm}`"))?;

        let child = Arc::new(Mutex::new(child));
        let disconnected = Arc::new(AtomicBool::new(false));

        let stdout_handle = Some(spawn_stream_reader(
            vm.clone(),
            SshStream::Stdout,
            stdout,
            sender.clone(),
            Arc::clone(&disconnected),
        ));
        let stderr_handle = Some(spawn_stream_reader(
            vm.clone(),
            SshStream::Stderr,
            stderr,
            sender.clone(),
            Arc::clone(&disconnected),
        ));
        let monitor_handle = Some(spawn_process_monitor(
            vm.clone(),
            Arc::clone(&child),
            sender,
            Arc::clone(&disconnected),
        ));

        Ok(Self {
            vm,
            child,
            stdin: Some(stdin),
            stdout_handle,
            stderr_handle,
            monitor_handle,
            disconnected,
        })
    }

    fn write_line(&mut self, line: &str) -> Result<(), String> {
        if let Some(stdin) = self.stdin.as_mut() {
            stdin
                .write_all(line.as_bytes())
                .and_then(|_| stdin.write_all(b"\n"))
                .and_then(|_| stdin.flush())
                .map_err(|err| format!("Failed to write to ssh for `{}`: {err}", self.vm))
        } else {
            Err(format!("SSH stdin is closed for `{}`.", self.vm))
        }
    }

    fn shutdown(&mut self, sender: &Sender<SshEvent>, vm: &str) {
        // Drop the stdin handle to signal EOF to the remote shell.
        self.stdin.take();

        let mut notified = false;

        match self.child.lock() {
            Ok(mut child) => match child.try_wait() {
                Ok(Some(status)) => {
                    notify_disconnect(vm, sender, &self.disconnected, status.code());
                    notified = true;
                }
                Ok(None) => {
                    let wait_result = child.kill().and_then(|_| child.wait());
                    match wait_result {
                        Ok(status) => {
                            notify_disconnect(vm, sender, &self.disconnected, status.code());
                            notified = true;
                        }
                        Err(err) => {
                            let _ = sender.try_send(SshEvent::ConnectionFailed {
                                vm: vm.to_string(),
                                error: format!("Failed to terminate ssh for `{vm}`: {err}"),
                            });
                        }
                    }
                }
                Err(err) => {
                    let _ = sender.try_send(SshEvent::ConnectionFailed {
                        vm: vm.to_string(),
                        error: format!("Failed to query ssh status for `{vm}`: {err}"),
                    });
                }
            },
            Err(_) => {
                notify_disconnect(vm, sender, &self.disconnected, None);
                notified = true;
            }
        }

        if let Some(handle) = self.stdout_handle.take() {
            let _ = handle.join();
        }
        if let Some(handle) = self.stderr_handle.take() {
            let _ = handle.join();
        }
        if let Some(handle) = self.monitor_handle.take() {
            let _ = handle.join();
        }

        if !notified {
            notify_disconnect(vm, sender, &self.disconnected, None);
        }
    }
}

fn spawn_stream_reader<R: Read + Send + 'static>(
    vm: String,
    stream: SshStream,
    reader: R,
    sender: Sender<SshEvent>,
    disconnected: Arc<AtomicBool>,
) -> thread::JoinHandle<()> {
    thread::Builder::new()
        .name(format!(
            "ssh-{}-{}",
            thread_label(&vm),
            match stream {
                SshStream::Stdout => "stdout",
                SshStream::Stderr => "stderr",
            }
        ))
        .spawn(move || {
            let mut reader = BufReader::new(reader);
            let mut buffer = Vec::with_capacity(1024);
            loop {
                buffer.clear();
                match reader.read_until(b'\n', &mut buffer) {
                    Ok(0) => {
                        notify_disconnect(&vm, &sender, &disconnected, None);
                        break;
                    }
                    Ok(_) => {
                        while buffer.ends_with(&[b'\n']) || buffer.ends_with(&[b'\r']) {
                            buffer.pop();
                        }
                        let line = String::from_utf8_lossy(&buffer).to_string();
                        if sender
                            .send_blocking(SshEvent::Output {
                                vm: vm.clone(),
                                stream,
                                line,
                            })
                            .is_err()
                        {
                            break;
                        }
                    }
                    Err(err) => {
                        let message = format!("ssh stream error: {err}");
                        let _ = sender.send_blocking(SshEvent::Output {
                            vm: vm.clone(),
                            stream,
                            line: message,
                        });
                        notify_disconnect(&vm, &sender, &disconnected, None);
                        break;
                    }
                }
            }
        })
        .expect("failed to spawn ssh stream reader")
}

fn spawn_process_monitor(
    vm: String,
    child: Arc<Mutex<Child>>,
    sender: Sender<SshEvent>,
    disconnected: Arc<AtomicBool>,
) -> thread::JoinHandle<()> {
    thread::Builder::new()
        .name(format!("ssh-{}-monitor", thread_label(&vm)))
        .spawn(move || {
            loop {
                let status = {
                    let mut child = match child.lock() {
                        Ok(child) => child,
                        Err(_) => {
                            notify_disconnect(&vm, &sender, &disconnected, None);
                            break;
                        }
                    };
                    match child.try_wait() {
                        Ok(status) => status,
                        Err(err) => {
                            let message = format!("ssh wait error: {err}");
                            let _ = sender.send_blocking(SshEvent::Output {
                                vm: vm.clone(),
                                stream: SshStream::Stderr,
                                line: message,
                            });
                            notify_disconnect(&vm, &sender, &disconnected, None);
                            break;
                        }
                    }
                };

                if let Some(status) = status {
                    notify_disconnect(&vm, &sender, &disconnected, status.code());
                    break;
                }

                if disconnected.load(Ordering::SeqCst) {
                    break;
                }

                thread::sleep(Duration::from_millis(250));
            }
        })
        .expect("failed to spawn ssh process monitor")
}

fn notify_disconnect(
    vm: &str,
    sender: &Sender<SshEvent>,
    flag: &Arc<AtomicBool>,
    exit_status: Option<i32>,
) {
    if flag.swap(true, Ordering::SeqCst) {
        return;
    }
    let _ = sender.send_blocking(SshEvent::Disconnected {
        vm: vm.to_string(),
        exit_status,
    });
}

fn thread_label(input: &str) -> String {
    let mut label: String = input
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '-' || *ch == '_')
        .collect();
    if label.len() > 12 {
        label.truncate(12);
    }
    if label.is_empty() {
        String::from("vm")
    } else {
        label
    }
}
