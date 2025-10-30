use std::fmt;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use castra::core::{
    diagnostics::Severity,
    events::{BootstrapStatus, BootstrapStepKind, BootstrapStepStatus, BootstrapTrigger, Event},
};
use castra_harness::{CommandStatus, FileDiff, FileDiffKind, HarnessEvent, PatchStatus, TodoEntry};
use chrono::{DateTime, Local};
use gpui::SharedString;

fn current_timestamp() -> SharedString {
    Local::now().format("%H:%M:%S").to_string().into()
}

#[derive(Clone)]
pub struct ChatMessage {
    timestamp: SharedString,
    speaker: SharedString,
    text: SharedString,
}

impl ChatMessage {
    pub fn new<S: Into<SharedString>, T: Into<SharedString>>(speaker: S, text: T) -> Self {
        Self {
            timestamp: current_timestamp(),
            speaker: speaker.into(),
            text: text.into(),
        }
    }

    pub fn timestamp(&self) -> &SharedString {
        &self.timestamp
    }

    pub fn speaker(&self) -> &SharedString {
        &self.speaker
    }

    pub fn text(&self) -> &SharedString {
        &self.text
    }
}

#[derive(Clone)]
pub struct Agent {
    id: String,
    status: String,
}

impl Agent {
    pub fn new(id: &str, status: &str) -> Self {
        Self {
            id: id.to_string(),
            status: status.to_string(),
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn status(&self) -> &str {
        &self.status
    }

    pub fn label(&self) -> String {
        self.id.to_uppercase()
    }

    pub fn set_status<T: Into<String>>(&mut self, status: T) {
        self.status = status.into();
    }
}

#[derive(Default)]
pub struct ChatState {
    messages: Vec<ChatMessage>,
}

impl ChatState {
    pub fn push_message(&mut self, message: ChatMessage) {
        self.messages.push(message);
    }

    pub fn messages(&self) -> &[ChatMessage] {
        &self.messages
    }
}

pub struct RosterState {
    agents: Vec<Agent>,
    active_agent: usize,
}

impl RosterState {
    pub fn agents(&self) -> &[Agent] {
        &self.agents
    }

    pub fn active_index(&self) -> usize {
        self.active_agent
    }

    pub fn active_agent(&self) -> &Agent {
        &self.agents[self.active_agent]
    }

    pub fn switch_to(&mut self, index: usize) -> bool {
        if index >= self.agents.len() || index == self.active_agent {
            return false;
        }
        self.active_agent = index;
        true
    }

    pub fn set_active_status<T: Into<String>>(&mut self, status: T) {
        if let Some(agent) = self.agents.get_mut(self.active_agent) {
            agent.set_status(status);
        }
    }
}

impl Default for RosterState {
    fn default() -> Self {
        Self {
            agents: vec![
                Agent::new("assist", "ONLINE"),
                Agent::new("research", "IDLE"),
                Agent::new("builder", "STANDBY"),
            ],
            active_agent: 0,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum AttentionLevel {
    Idle,
    Progress,
    Success,
    Error,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum VmPhase {
    Pending,
    Planned,
    OverlayPrepared,
    Launching,
    Bootstrapping,
    Ready,
    Failed,
}

impl VmPhase {
    pub fn label(self) -> &'static str {
        match self {
            VmPhase::Pending => "PENDING",
            VmPhase::Planned => "PLANNED",
            VmPhase::OverlayPrepared => "OVERLAY READY",
            VmPhase::Launching => "LAUNCHING",
            VmPhase::Bootstrapping => "BOOTSTRAPPING",
            VmPhase::Ready => "READY",
            VmPhase::Failed => "FAILED",
        }
    }
}

impl fmt::Display for VmPhase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

#[derive(Clone)]
pub struct VirtualMachine {
    name: String,
    phase: VmPhase,
    attention: AttentionLevel,
    detail: String,
}

impl VirtualMachine {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            phase: VmPhase::Pending,
            attention: AttentionLevel::Idle,
            detail: "Awaiting events...".to_string(),
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn phase(&self) -> VmPhase {
        self.phase
    }

    pub fn attention(&self) -> AttentionLevel {
        self.attention
    }

    pub fn detail(&self) -> &str {
        &self.detail
    }

    pub fn set_state<T: Into<String>>(
        &mut self,
        phase: VmPhase,
        attention: AttentionLevel,
        detail: T,
    ) {
        self.phase = phase;
        self.attention = attention;
        self.detail = detail.into();
    }
}

pub struct VmFleetState {
    vms: Vec<VirtualMachine>,
    focused: Option<usize>,
}

impl Default for VmFleetState {
    fn default() -> Self {
        Self {
            vms: Vec::new(),
            focused: None,
        }
    }
}

impl VmFleetState {
    pub fn virtual_machines(&self) -> &[VirtualMachine] {
        &self.vms
    }

    pub fn focused_index(&self) -> Option<usize> {
        self.focused
    }

    pub fn focused_vm(&self) -> Option<&VirtualMachine> {
        self.focused.and_then(|index| self.vms.get(index))
    }

    pub fn reset(&mut self) {
        self.vms.clear();
        self.focused = None;
    }

    pub fn ensure_vm(&mut self, name: &str) -> &mut VirtualMachine {
        if let Some(index) = self.vms.iter().position(|vm| vm.name == name) {
            &mut self.vms[index]
        } else {
            self.vms.push(VirtualMachine::new(name));
            let index = self.vms.len() - 1;
            if self.focused.is_none() {
                self.focused = Some(index);
            }
            self.vms.last_mut().expect("new VM inserted")
        }
    }

    pub fn update_vm<T: Into<String>>(
        &mut self,
        name: &str,
        phase: VmPhase,
        attention: AttentionLevel,
        detail: T,
    ) {
        let vm = self.ensure_vm(name);
        vm.set_state(phase, attention, detail);
    }

    #[allow(dead_code)]
    pub fn focus_first(&mut self) -> Option<usize> {
        if self.vms.is_empty() {
            self.focused = None;
            return None;
        }
        if self.focused.is_some() {
            return None;
        }
        self.focused = Some(0);
        Some(0)
    }

    #[allow(dead_code)]
    pub fn focus_vm_at(&mut self, index: usize) -> Option<usize> {
        if index >= self.vms.len() {
            return None;
        }
        if self.focused == Some(index) {
            return None;
        }
        self.focused = Some(index);
        Some(index)
    }

    pub fn focus_next(&mut self) -> Option<usize> {
        if self.vms.is_empty() {
            self.focused = None;
            return None;
        }
        let next = match self.focused {
            Some(current) => (current + 1) % self.vms.len(),
            None => 0,
        };
        if self.focused == Some(next) {
            return None;
        }
        self.focused = Some(next);
        Some(next)
    }

    pub fn focus_prev(&mut self) -> Option<usize> {
        if self.vms.is_empty() {
            self.focused = None;
            return None;
        }
        let len = self.vms.len();
        let prev = match self.focused {
            Some(current) => (current + len - 1) % len,
            None => len - 1,
        };
        if self.focused == Some(prev) {
            return None;
        }
        self.focused = Some(prev);
        Some(prev)
    }

    pub fn counts(&self) -> VmCounts {
        let mut ready = 0usize;
        let mut failed = 0usize;

        for vm in &self.vms {
            match vm.phase {
                VmPhase::Ready => ready += 1,
                VmPhase::Failed => failed += 1,
                _ => {}
            }
        }

        VmCounts {
            total: self.vms.len(),
            ready,
            failed,
        }
    }
}

#[derive(Default, Clone, Copy)]
pub struct VmCounts {
    pub total: usize,
    pub ready: usize,
    pub failed: usize,
}

impl VmCounts {
    pub fn in_progress(self) -> usize {
        self.total.saturating_sub(self.ready + self.failed)
    }
}

#[allow(dead_code)]
#[derive(Clone)]
pub struct RuntimePaths {
    state_root: PathBuf,
    log_root: PathBuf,
}

#[allow(dead_code)]
impl RuntimePaths {
    pub fn state_root(&self) -> &PathBuf {
        &self.state_root
    }

    pub fn log_root(&self) -> &PathBuf {
        &self.log_root
    }
}

#[derive(Clone)]
pub enum UpLifecycle {
    Idle,
    Running {
        started_at: DateTime<Local>,
    },
    Success {
        started_at: DateTime<Local>,
        completed_at: DateTime<Local>,
    },
    Failed {
        started_at: Option<DateTime<Local>>,
        reason: String,
    },
}

impl Default for UpLifecycle {
    fn default() -> Self {
        Self::Idle
    }
}

pub struct UpState {
    lifecycle: UpLifecycle,
    vm_fleet: VmFleetState,
    broker_port: Option<u16>,
    last_error: Option<String>,
    runtime_paths: Option<RuntimePaths>,
    shutdown_in_progress: bool,
}

impl Default for UpState {
    fn default() -> Self {
        Self {
            lifecycle: UpLifecycle::Idle,
            vm_fleet: VmFleetState::default(),
            broker_port: None,
            last_error: None,
            runtime_paths: None,
            shutdown_in_progress: false,
        }
    }
}

impl UpState {
    pub fn is_running(&self) -> bool {
        matches!(self.lifecycle, UpLifecycle::Running { .. })
    }

    pub fn start(&mut self) -> bool {
        if self.is_running() {
            return false;
        }
        self.lifecycle = UpLifecycle::Running {
            started_at: Local::now(),
        };
        self.vm_fleet.reset();
        self.broker_port = None;
        self.last_error = None;
        self.runtime_paths = None;
        self.shutdown_in_progress = false;
        true
    }

    pub fn mark_success(&mut self) {
        let (started_at, prev_error) = match &self.lifecycle {
            UpLifecycle::Running { started_at } => (*started_at, self.last_error.clone()),
            UpLifecycle::Success { started_at, .. } => (*started_at, self.last_error.clone()),
            UpLifecycle::Failed { started_at, .. } => (
                started_at.unwrap_or_else(Local::now),
                self.last_error.clone(),
            ),
            UpLifecycle::Idle => (Local::now(), self.last_error.clone()),
        };

        self.lifecycle = UpLifecycle::Success {
            started_at,
            completed_at: Local::now(),
        };
        self.last_error = prev_error;
    }

    pub fn mark_failure<T: Into<String>>(&mut self, reason: T) {
        let started_at = match &self.lifecycle {
            UpLifecycle::Running { started_at } => Some(*started_at),
            UpLifecycle::Success { started_at, .. } => Some(*started_at),
            UpLifecycle::Failed { started_at, .. } => *started_at,
            UpLifecycle::Idle => None,
        };
        let reason = reason.into();
        self.lifecycle = UpLifecycle::Failed {
            started_at,
            reason: reason.clone(),
        };
        self.last_error = Some(reason);
    }

    pub fn vm_fleet(&self) -> &VmFleetState {
        &self.vm_fleet
    }

    pub fn vm_fleet_mut(&mut self) -> &mut VmFleetState {
        &mut self.vm_fleet
    }

    pub fn set_broker_port(&mut self, port: u16) {
        self.broker_port = Some(port);
    }

    pub fn note_error<T: Into<String>>(&mut self, message: T) {
        self.last_error = Some(message.into());
    }

    pub fn counts(&self) -> VmCounts {
        self.vm_fleet.counts()
    }

    pub fn set_runtime_paths(&mut self, state_root: PathBuf, log_root: PathBuf) {
        self.runtime_paths = Some(RuntimePaths {
            state_root,
            log_root,
        });
    }

    pub fn clear_runtime_paths(&mut self) {
        self.runtime_paths = None;
    }

    #[allow(dead_code)]
    pub fn runtime_paths(&self) -> Option<&RuntimePaths> {
        self.runtime_paths.as_ref()
    }

    pub fn mark_shutdown_started(&mut self) {
        self.shutdown_in_progress = true;
    }

    pub fn mark_shutdown_complete(&mut self) {
        self.shutdown_in_progress = false;
        self.clear_runtime_paths();
    }

    pub fn shutdown_in_progress(&self) -> bool {
        self.shutdown_in_progress
    }

    pub fn status_line(&self) -> String {
        let counts = self.counts();
        match &self.lifecycle {
            UpLifecycle::Idle => "UP idle".to_string(),
            UpLifecycle::Running { .. } => {
                let mut parts = vec![format!(
                    "UP running: {} ready / {} total",
                    counts.ready, counts.total
                )];
                let in_progress = counts.in_progress();
                if in_progress > 0 {
                    parts.push(format!("{in_progress} in progress"));
                }
                if counts.failed > 0 {
                    parts.push(format!("{} failed", counts.failed));
                }
                if let Some(port) = self.broker_port {
                    parts.push(format!("broker on :{port}"));
                }
                if let Some(error) = &self.last_error {
                    parts.push(format!("last error: {error}"));
                }
                parts.join(" • ")
            }
            UpLifecycle::Success {
                started_at,
                completed_at,
            } => {
                let elapsed = (*completed_at - *started_at).num_milliseconds();
                let duration = if elapsed <= 0 {
                    "<1s".to_string()
                } else {
                    format!("{:.1}s", (elapsed as f64) / 1000.0)
                };
                let total = counts.total.max(counts.ready);
                format!(
                    "UP complete in {duration} • {}/{} ready",
                    counts.ready, total
                )
            }
            UpLifecycle::Failed { reason, .. } => format!("UP failed: {reason}"),
        }
    }
}

#[derive(Default)]
pub struct UiState {
    sidebar_visible: bool,
    toasts: Vec<Toast>,
}

impl UiState {
    pub fn sidebar_visible(&self) -> bool {
        self.sidebar_visible
    }

    pub fn toggle_sidebar(&mut self) {
        self.sidebar_visible = !self.sidebar_visible;
    }

    pub fn push_toast<T: Into<String>>(&mut self, message: T) {
        self.prune_toasts();
        self.toasts.push(Toast::new(message));
    }

    pub fn collect_active_toasts(&mut self) -> Vec<String> {
        self.prune_toasts();
        self.toasts
            .iter()
            .map(|toast| toast.message.clone())
            .collect()
    }

    fn prune_toasts(&mut self) {
        let now = Instant::now();
        self.toasts.retain(|toast| !toast.is_expired(now));
    }
}

const TOAST_TTL: Duration = Duration::from_secs(3);

struct Toast {
    message: String,
    created_at: Instant,
}

impl Toast {
    fn new<T: Into<String>>(message: T) -> Self {
        Self {
            message: message.into(),
            created_at: Instant::now(),
        }
    }

    fn is_expired(&self, now: Instant) -> bool {
        now.duration_since(self.created_at) > TOAST_TTL
    }
}

pub struct AppState {
    chat: ChatState,
    roster: RosterState,
    up: UpState,
    ui: UiState,
    codex_thread_id: Option<String>,
    config_path: Option<PathBuf>,
}

impl AppState {
    pub fn new() -> Self {
        let mut state = Self {
            chat: ChatState::default(),
            roster: RosterState::default(),
            up: UpState::default(),
            ui: UiState::default(),
            codex_thread_id: None,
            config_path: None,
        };
        state.push_system_message("Welcome to Castra. Type /help to discover commands.");
        state.push_system_message("Run /up to launch the bootstrap-quickstart workspace.");
        state
    }

    pub fn chat(&self) -> &ChatState {
        &self.chat
    }

    pub fn roster(&self) -> &RosterState {
        &self.roster
    }

    pub fn vm_fleet(&self) -> &VmFleetState {
        self.up.vm_fleet()
    }

    pub fn focused_vm_name(&self) -> Option<String> {
        self.up
            .vm_fleet()
            .focused_vm()
            .map(|vm| vm.name().to_string())
    }

    pub fn focused_vm_label(&self) -> Option<String> {
        self.focused_vm_name().map(|name| name.to_uppercase())
    }

    #[allow(dead_code)]
    pub fn focus_vm_at(&mut self, index: usize) -> Option<String> {
        let new_index = self.up.vm_fleet_mut().focus_vm_at(index)?;
        self.up
            .vm_fleet()
            .virtual_machines()
            .get(new_index)
            .map(|vm| vm.name().to_string())
    }

    pub fn focus_next_vm(&mut self) -> Option<String> {
        let new_index = self.up.vm_fleet_mut().focus_next()?;
        self.up
            .vm_fleet()
            .virtual_machines()
            .get(new_index)
            .map(|vm| vm.name().to_string())
    }

    pub fn focus_prev_vm(&mut self) -> Option<String> {
        let new_index = self.up.vm_fleet_mut().focus_prev()?;
        self.up
            .vm_fleet()
            .virtual_machines()
            .get(new_index)
            .map(|vm| vm.name().to_string())
    }

    pub fn resolve_vm_name(&self, candidate: &str) -> Option<String> {
        let needle = candidate.trim();
        if needle.is_empty() {
            return None;
        }
        self.up
            .vm_fleet()
            .virtual_machines()
            .iter()
            .find(|vm| vm.name().eq_ignore_ascii_case(needle))
            .map(|vm| vm.name().to_string())
    }

    pub fn push_toast<T: Into<String>>(&mut self, message: T) {
        self.ui.push_toast(message);
    }

    pub fn collect_active_toasts(&mut self) -> Vec<String> {
        self.ui.collect_active_toasts()
    }

    pub fn toggle_sidebar(&mut self) {
        self.ui.toggle_sidebar();
    }

    pub fn sidebar_visible(&self) -> bool {
        self.ui.sidebar_visible()
    }

    pub fn active_agent_label(&self) -> String {
        self.roster.active_agent().label()
    }

    pub fn active_agent_index(&self) -> usize {
        self.roster.active_index()
    }

    pub fn switch_agent(&mut self, index: usize) -> bool {
        self.roster.switch_to(index)
    }

    pub fn push_message<S: Into<String>, T: Into<String>>(&mut self, speaker: S, text: T) {
        let speaker = speaker.into();
        let text = text.into();
        self.chat.push_message(ChatMessage::new(speaker, text));
    }

    pub fn push_system_message<T: Into<String>>(&mut self, text: T) {
        self.push_message("SYSTEM", text);
    }

    pub fn push_user_command(&mut self, text: &str) {
        self.push_message("USER", text.to_string());
    }

    pub fn push_user_entry(&mut self, text: &str) {
        let target = self.roster.active_agent().label();
        let speaker = format!("USER→{}", target);
        self.push_message(speaker, text.to_string());
    }

    pub fn push_agent_echo(&mut self, text: &str) {
        let label = self.roster.active_agent().label();
        self.push_message(label, format!("You said: {}", text));
    }

    pub fn codex_thread_id(&self) -> Option<String> {
        self.codex_thread_id.clone()
    }

    pub fn apply_harness_event(&mut self, event: &HarnessEvent) {
        match event {
            HarnessEvent::ThreadStarted { thread_id } => {
                self.codex_thread_id = Some(thread_id.clone());
                self.push_system_message(format!("Codex thread ready ({thread_id})"));
            }
            HarnessEvent::AgentMessage { text } => {
                self.push_message("CODEX", text.clone());
            }
            HarnessEvent::Reasoning { text } => {
                self.push_message("CODEX⋯", text.clone());
            }
            HarnessEvent::CommandProgress {
                command,
                output,
                status,
                exit_code,
            } => {
                let status_label = match status {
                    CommandStatus::InProgress => "running",
                    CommandStatus::Completed => "completed",
                    CommandStatus::Failed => "failed",
                };
                let mut message = format!("Codex command {status_label}: {command}");
                if let Some(code) = exit_code {
                    message.push_str(&format!(" (exit {code})"));
                }
                self.push_system_message(message);
                if !output.is_empty() {
                    self.push_message("CODEX·CMD", output.clone());
                }
            }
            HarnessEvent::FileChange { changes, status } => {
                let status_label = match status {
                    PatchStatus::Completed => "applied",
                    PatchStatus::Failed => "failed",
                };
                let summary = render_file_changes(changes);
                self.push_system_message(format!("Codex file changes {status_label}: {summary}"));
            }
            HarnessEvent::TodoList { items } => {
                let summary = render_todo_list(items);
                self.push_system_message(format!("Codex TODO: {summary}"));
            }
            HarnessEvent::Usage {
                prompt_tokens,
                cached_tokens,
                completion_tokens,
            } => {
                self.push_system_message(format!(
                    "Codex usage — prompt: {prompt_tokens}, cached: {cached_tokens}, completion: {completion_tokens}"
                ));
            }
            HarnessEvent::Failure { message } => {
                self.push_system_message(format!("Codex failure: {message}"));
            }
        }
    }

    pub fn agent_index_by_id(&self, id: &str) -> Option<usize> {
        self.roster
            .agents()
            .iter()
            .position(|agent| agent.id().eq_ignore_ascii_case(id))
    }

    pub fn begin_up_operation(&mut self) -> Result<(), &'static str> {
        if self.up.shutdown_in_progress() {
            return Err("Shutdown is in progress; wait for cleanup to finish.");
        }
        if !self.up.start() {
            return Err("An /up operation is already in progress.");
        }
        self.roster.set_active_status("RUNNING");
        Ok(())
    }

    pub fn complete_up_success(&mut self) {
        self.up.mark_success();
        self.roster.set_active_status("ONLINE");
    }

    pub fn complete_up_failure<T: Into<String>>(&mut self, reason: T) {
        self.up.mark_failure(reason.into());
        self.roster.set_active_status("ERROR");
    }

    pub fn record_runtime_paths(&mut self, state_root: PathBuf, log_root: PathBuf) {
        self.up.set_runtime_paths(state_root, log_root);
    }

    #[allow(dead_code)]
    pub fn runtime_paths(&self) -> Option<&RuntimePaths> {
        self.up.runtime_paths()
    }

    pub fn mark_shutdown_started(&mut self) {
        self.up.mark_shutdown_started();
    }

    pub fn mark_shutdown_complete(&mut self) {
        self.up.mark_shutdown_complete();
        self.roster.set_active_status("ONLINE");
    }

    pub fn shutdown_in_progress(&self) -> bool {
        self.up.shutdown_in_progress()
    }

    pub fn set_config_path(&mut self, path: PathBuf) {
        self.config_path = Some(path);
    }

    #[allow(dead_code)]
    pub fn clear_config_path(&mut self) {
        self.config_path = None;
    }

    #[allow(dead_code)]
    pub fn config_path(&self) -> Option<&PathBuf> {
        self.config_path.as_ref()
    }

    pub fn handle_up_event(&mut self, event: &Event) -> Option<String> {
        use AttentionLevel::*;
        use VmPhase::*;

        match event {
            Event::Message { severity, text } => {
                let tag = match severity {
                    Severity::Info => "INFO",
                    Severity::Warning => "WARN",
                    Severity::Error => "ERROR",
                };
                if matches!(severity, Severity::Error) {
                    self.up.note_error(text.clone());
                }
                Some(format!("[{tag}] {text}"))
            }
            Event::BootstrapPlanned {
                vm, action, reason, ..
            } => {
                let attention = if action.is_error() { Error } else { Progress };
                let detail = format!("Plan {} ({reason})", action.describe());
                self.up
                    .vm_fleet_mut()
                    .update_vm(vm, Planned, attention, detail.clone());
                if action.is_error() {
                    self.up.note_error(detail.clone());
                }
                Some(format!("{vm}: bootstrap plan {}", action.describe()))
            }
            Event::OverlayPrepared { vm, overlay_path } => {
                self.up.vm_fleet_mut().update_vm(
                    vm,
                    OverlayPrepared,
                    Progress,
                    format!("Overlay ready at {}", overlay_path.display()),
                );
                Some(format!("{vm}: overlay prepared"))
            }
            Event::VmLaunched { vm, pid } => {
                self.up.vm_fleet_mut().update_vm(
                    vm,
                    Launching,
                    Progress,
                    format!("VM launched (pid {pid})"),
                );
                Some(format!("{vm}: VM launched (pid {pid})"))
            }
            Event::BootstrapStarted { vm, trigger, .. } => {
                let trigger_text = format_trigger(trigger);
                self.up.vm_fleet_mut().update_vm(
                    vm,
                    Bootstrapping,
                    Progress,
                    format!("Bootstrap started ({trigger_text})"),
                );
                Some(format!("{vm}: bootstrap started ({trigger_text})"))
            }
            Event::BootstrapStep {
                vm,
                step,
                status,
                duration_ms,
                detail,
            } => {
                let text = format_step(step, status, *duration_ms, detail.as_deref());
                let attention = if matches!(status, BootstrapStepStatus::Failed) {
                    self.up.note_error(text.clone());
                    Error
                } else {
                    Progress
                };
                self.up
                    .vm_fleet_mut()
                    .update_vm(vm, Bootstrapping, attention, text.clone());
                Some(format!("{vm}: {text}"))
            }
            Event::BootstrapCompleted {
                vm,
                status,
                duration_ms,
                ..
            } => {
                let text = match status {
                    BootstrapStatus::Success => {
                        format!("Bootstrap succeeded in {} ms", duration_ms)
                    }
                    BootstrapStatus::NoOp => {
                        format!("Bootstrap skipped (noop) in {} ms", duration_ms)
                    }
                };
                self.up
                    .vm_fleet_mut()
                    .update_vm(vm, Ready, Success, text.clone());
                Some(format!("{vm}: {text}"))
            }
            Event::BootstrapFailed {
                vm,
                duration_ms,
                error,
            } => {
                let text = format!("Bootstrap failed after {} ms: {error}", duration_ms);
                self.up
                    .vm_fleet_mut()
                    .update_vm(vm, Failed, Error, text.clone());
                self.up.note_error(format!("{vm}: {error}"));
                Some(format!("{vm}: {text}"))
            }
            Event::BrokerStarted { pid, port } => {
                self.up.set_broker_port(*port);
                Some(format!("Broker started (pid {pid}) on port {port}"))
            }
            Event::BrokerStopped { changed } => {
                if *changed {
                    Some("Broker stopped".to_string())
                } else {
                    Some("Broker already offline".to_string())
                }
            }
            _ => None,
        }
    }

    pub fn up_status_line(&self) -> String {
        self.up.status_line()
    }
}

fn format_step(
    step: &BootstrapStepKind,
    status: &BootstrapStepStatus,
    duration_ms: u64,
    detail: Option<&str>,
) -> String {
    let status_label = match status {
        BootstrapStepStatus::Success => "success",
        BootstrapStepStatus::Skipped => "skipped",
        BootstrapStepStatus::Failed => "failed",
    };

    let mut text = format!("{:?} {status_label} ({duration_ms} ms)", step);
    if let Some(detail) = detail {
        if !detail.is_empty() {
            text.push_str(": ");
            text.push_str(detail);
        }
    }
    text
}

fn format_trigger(trigger: &BootstrapTrigger) -> String {
    match trigger {
        BootstrapTrigger::Always => "always".to_string(),
        BootstrapTrigger::Auto => "auto".to_string(),
    }
}

fn render_file_changes(changes: &[FileDiff]) -> String {
    if changes.is_empty() {
        return "none".to_string();
    }

    changes
        .iter()
        .map(|diff| format!("{} {}", describe_diff_kind(&diff.kind), diff.path))
        .collect::<Vec<_>>()
        .join(" • ")
}

fn describe_diff_kind(kind: &FileDiffKind) -> &'static str {
    match kind {
        FileDiffKind::Add => "added",
        FileDiffKind::Delete => "removed",
        FileDiffKind::Update => "updated",
    }
}

fn render_todo_list(items: &[TodoEntry]) -> String {
    if items.is_empty() {
        return "none".to_string();
    }

    items
        .iter()
        .map(|item| {
            let status = if item.completed { 'x' } else { ' ' };
            format!("[{status}] {}", item.text)
        })
        .collect::<Vec<_>>()
        .join(" • ")
}
