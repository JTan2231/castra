use std::fmt;

use castra::core::{
    diagnostics::Severity,
    events::{BootstrapStatus, BootstrapStepKind, BootstrapStepStatus, BootstrapTrigger, Event},
};
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

#[derive(Default)]
pub struct VmFleetState {
    vms: Vec<VirtualMachine>,
}

impl VmFleetState {
    pub fn virtual_machines(&self) -> &[VirtualMachine] {
        &self.vms
    }

    pub fn reset(&mut self) {
        self.vms.clear();
    }

    pub fn ensure_vm(&mut self, name: &str) -> &mut VirtualMachine {
        if let Some(index) = self.vms.iter().position(|vm| vm.name == name) {
            &mut self.vms[index]
        } else {
            self.vms.push(VirtualMachine::new(name));
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
}

impl Default for UpState {
    fn default() -> Self {
        Self {
            lifecycle: UpLifecycle::Idle,
            vm_fleet: VmFleetState::default(),
            broker_port: None,
            last_error: None,
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
}

impl UiState {
    pub fn sidebar_visible(&self) -> bool {
        self.sidebar_visible
    }

    pub fn toggle_sidebar(&mut self) {
        self.sidebar_visible = !self.sidebar_visible;
    }
}

pub struct AppState {
    chat: ChatState,
    roster: RosterState,
    up: UpState,
    ui: UiState,
}

impl AppState {
    pub fn new() -> Self {
        let mut state = Self {
            chat: ChatState::default(),
            roster: RosterState::default(),
            up: UpState::default(),
            ui: UiState::default(),
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

    pub fn agent_index_by_id(&self, id: &str) -> Option<usize> {
        self.roster
            .agents()
            .iter()
            .position(|agent| agent.id().eq_ignore_ascii_case(id))
    }

    pub fn begin_up_operation(&mut self) -> Result<(), &'static str> {
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
