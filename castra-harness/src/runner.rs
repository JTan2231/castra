use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, ExitStatus, Stdio};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use async_channel::{Receiver, Sender};

use crate::config::{HarnessConfig, TurnRequest};
use crate::error::HarnessError;
use crate::session::{SessionState, SessionUpdate};
use crate::stream;
use crate::translator::{self, HarnessEvent};

pub struct CodexSession {
    config: HarnessConfig,
}

impl CodexSession {
    pub fn new(config: HarnessConfig) -> Self {
        Self { config }
    }

    pub fn run_turn(&self, request: TurnRequest) -> Result<TurnHandle, HarnessError> {
        let binary = self.config.binary_path().to_path_buf();
        let mut command = Command::new(binary);
        command.arg("exec");
        command.arg("--json");

        let effective_model = request
            .model()
            .map(str::to_owned)
            .or_else(|| self.config.model().map(str::to_owned));
        if let Some(model) = effective_model.as_ref() {
            command.arg("--model");
            command.arg(model);
        }

        let effective_resume = request
            .resume_thread()
            .map(str::to_owned)
            .or_else(|| self.config.default_resume_thread().map(str::to_owned));
        if let Some(resume) = effective_resume.as_ref() {
            command.arg("resume");
            command.arg(resume);
        }

        command.arg("-");

        if let Some(dir) = self.config.working_dir() {
            command.current_dir(dir);
        }

        for (key, value) in self.config.env() {
            command.env(key, value);
        }

        command.stdin(Stdio::piped());
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());

        let mut child = command.spawn().map_err(HarnessError::Spawn)?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| HarnessError::process_failure(None, "stdout unavailable"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| HarnessError::process_failure(None, "stderr unavailable"))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| HarnessError::process_failure(None, "stdin unavailable"))?;

        let state = Arc::new(TurnState::new(child));
        let (sender, receiver) = async_channel::unbounded();

        let runner_state = Arc::clone(&state);
        let join_handle = thread::spawn(move || {
            run_codex_process(request, stdin, stdout, stderr, runner_state, sender)
        });

        state.store_join(join_handle);

        Ok(TurnHandle { receiver, state })
    }
}

pub struct TurnHandle {
    receiver: Receiver<HarnessEvent>,
    state: Arc<TurnState>,
}

impl TurnHandle {
    pub fn events(&self) -> Receiver<HarnessEvent> {
        self.receiver.clone()
    }

    pub fn cancel(&self) -> Result<(), HarnessError> {
        self.state.kill_child()
    }

    pub fn wait(&self) -> Result<(), HarnessError> {
        if let Some(join) = self.state.take_join() {
            match join.join() {
                Ok(result) => result,
                Err(_) => Err(HarnessError::process_failure(
                    None,
                    "harness thread panicked",
                )),
            }
        } else {
            Ok(())
        }
    }
}

impl Drop for TurnHandle {
    fn drop(&mut self) {
        if let Some(join) = self.state.take_join() {
            let _ = join.join();
        }
    }
}

struct TurnState {
    child: Mutex<Option<Child>>,
    join: Mutex<Option<JoinHandle<Result<(), HarnessError>>>>,
}

impl TurnState {
    fn new(child: Child) -> Self {
        Self {
            child: Mutex::new(Some(child)),
            join: Mutex::new(None),
        }
    }

    fn store_join(&self, handle: JoinHandle<Result<(), HarnessError>>) {
        let mut guard = self.join.lock().expect("turn join mutex poisoned");
        *guard = Some(handle);
    }

    fn take_join(&self) -> Option<JoinHandle<Result<(), HarnessError>>> {
        let mut guard = self.join.lock().expect("turn join mutex poisoned");
        guard.take()
    }

    fn kill_child(&self) -> Result<(), HarnessError> {
        let mut guard = self.child.lock().expect("turn child mutex poisoned");
        if let Some(child) = guard.as_mut() {
            match child.kill() {
                Ok(()) => Ok(()),
                Err(err) if err.kind() == std::io::ErrorKind::InvalidInput => Ok(()),
                Err(err) => Err(HarnessError::Io(err)),
            }
        } else {
            Ok(())
        }
    }

    fn take_child(&self) -> Option<Child> {
        let mut guard = self.child.lock().expect("turn child mutex poisoned");
        guard.take()
    }
}

fn run_codex_process(
    request: TurnRequest,
    mut stdin: ChildStdin,
    stdout: ChildStdout,
    stderr: ChildStderr,
    state: Arc<TurnState>,
    sender: Sender<HarnessEvent>,
) -> Result<(), HarnessError> {
    write_prompt(&mut stdin, request.prompt())?;
    drop(stdin);

    let stderr_buffer = Arc::new(Mutex::new(String::new()));
    let stderr_collector = Arc::clone(&stderr_buffer);
    let stderr_handle = thread::spawn(move || collect_stderr(stderr, stderr_collector));

    let result = pump_stdout(stdout, sender.clone());

    let stderr_joined = match stderr_handle.join() {
        Ok(Ok(())) => Ok(()),
        Ok(Err(err)) => Err(err),
        Err(_) => Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "stderr collector panicked",
        )),
    };

    if let Err(err) = stderr_joined {
        return Err(HarnessError::Io(err));
    }

    let exit_status = wait_for_child(&state)?;
    if !exit_status.map_or(false, |status| status.success()) {
        let stderr_text = stderr_buffer
            .lock()
            .expect("stderr buffer poisoned")
            .clone();
        let message = process_failure_message(exit_status, stderr_text);
        let _ = sender.send_blocking(HarnessEvent::Failure {
            message: message.clone(),
        });
        return Err(HarnessError::process_failure(exit_status, message));
    }

    result
}

fn write_prompt(stdin: &mut ChildStdin, prompt: &str) -> Result<(), HarnessError> {
    stdin.write_all(prompt.as_bytes())?;
    stdin.flush()?;
    Ok(())
}

fn collect_stderr(stderr: ChildStderr, buffer: Arc<Mutex<String>>) -> Result<(), std::io::Error> {
    let mut reader = BufReader::new(stderr);
    let mut line = String::new();
    loop {
        line.clear();
        let read = reader.read_line(&mut line)?;
        if read == 0 {
            break;
        }
        let trimmed = line.trim_end_matches(['\n', '\r']);
        if trimmed.is_empty() {
            continue;
        }
        let mut guard = buffer.lock().expect("stderr buffer poisoned");
        if !guard.is_empty() {
            guard.push('\n');
        }
        guard.push_str(trimmed);
    }
    Ok(())
}

fn pump_stdout(stdout: ChildStdout, sender: Sender<HarnessEvent>) -> Result<(), HarnessError> {
    let mut reader = BufReader::new(stdout);
    let mut session = SessionState::new();
    let mut line = String::new();

    while reader.read_line(&mut line)? != 0 {
        let raw = line.trim_end_matches(['\n', '\r']).to_string();
        line.clear();
        if raw.is_empty() {
            continue;
        }

        match stream::decode_line(&raw) {
            Ok(event) => {
                let updates = session.apply(event);
                forward_updates(updates, &sender)?;
            }
            Err(err) => {
                let message = format!("Failed to decode Codex event: {err}");
                let _ = sender.send_blocking(HarnessEvent::Failure {
                    message: message.clone(),
                });
                return Err(err);
            }
        }
    }

    Ok(())
}

fn forward_updates(
    updates: Vec<SessionUpdate>,
    sender: &Sender<HarnessEvent>,
) -> Result<(), HarnessError> {
    for update in updates {
        for event in translator::translate(update) {
            if sender.send_blocking(event).is_err() {
                return Ok(());
            }
        }
    }
    Ok(())
}

fn wait_for_child(state: &Arc<TurnState>) -> Result<Option<ExitStatus>, HarnessError> {
    if let Some(mut child) = state.take_child() {
        let status = child.wait()?;
        Ok(Some(status))
    } else {
        Ok(None)
    }
}

fn process_failure_message(status: Option<ExitStatus>, stderr: String) -> String {
    match status {
        Some(status) => {
            if stderr.is_empty() {
                format!("codex exited with status {status}")
            } else {
                format!("codex exited with status {status}: {stderr}")
            }
        }
        None => {
            if stderr.is_empty() {
                "codex process terminated".to_string()
            } else {
                format!("codex process terminated: {stderr}")
            }
        }
    }
}
