use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use castra::core::{
    diagnostics::Severity,
    events::{
        BootstrapPlanSsh, BootstrapStatus, BootstrapStepKind, BootstrapStepStatus,
        BootstrapTrigger, Event,
    },
};
use castra_harness::{
    CommandStatus, FileDiff, FileDiffKind, HarnessEvent, PatchStatus, TodoEntry, VmEndpoint,
};
use chrono::{DateTime, Local};
use gpui::{ListAlignment, ListOffset, ListState, Pixels, SharedString, px};

use crate::transcript::TranscriptWriter;

const VIZIER_AGENT_ID: &str = "vizier";
const COLLAPSED_PREVIEW_MAX_CHARS: usize = 80;
const DEFAULT_LOG_SOFT_LIMIT: usize = 500;

fn current_timestamp() -> SharedString {
    Local::now().format("%H:%M:%S").to_string().into()
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TokenUsageTotals {
    prompt: i64,
    cached: i64,
    completion: i64,
}

impl TokenUsageTotals {
    pub fn add(&mut self, prompt: i64, cached: i64, completion: i64) {
        self.prompt += prompt;
        self.cached += cached;
        self.completion += completion;
    }

    pub fn total(&self) -> i64 {
        self.prompt + self.cached + self.completion
    }

    pub fn is_empty(&self) -> bool {
        self.prompt == 0 && self.cached == 0 && self.completion == 0
    }

    pub fn summary(&self, label: &str) -> String {
        format!(
            "{label}: {} tok (prompt {}, cached {}, completion {})",
            self.total(),
            self.prompt,
            self.cached,
            self.completion
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MessageKind {
    System,
    Reasoning,
    Tool,
    VizierCommand,
    User,
    Agent,
    Other,
}

impl MessageKind {
    fn from_speaker(speaker: &str) -> Self {
        let normalized = speaker.trim();
        if normalized.eq_ignore_ascii_case("SYSTEM") {
            MessageKind::System
        } else if normalized.contains('⋯') {
            MessageKind::Reasoning
        } else if normalized.contains("·CMD") {
            MessageKind::Tool
        } else if normalized.eq_ignore_ascii_case("VIZIER·SYS") {
            MessageKind::VizierCommand
        } else if normalized.starts_with("USER") {
            MessageKind::User
        } else if normalized.contains("CODEX") || normalized.contains("VIZIER") {
            MessageKind::Agent
        } else {
            MessageKind::Other
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            MessageKind::System => "System",
            MessageKind::Reasoning => "Reasoning",
            MessageKind::Tool => "Tool Output",
            MessageKind::VizierCommand => "Vizier Command",
            MessageKind::User => "User",
            MessageKind::Agent => "Agent",
            MessageKind::Other => "Message",
        }
    }

    pub fn is_collapsible(&self) -> bool {
        matches!(self, MessageKind::Reasoning | MessageKind::Tool)
    }

    pub fn slug(&self) -> &'static str {
        match self {
            MessageKind::System => "system",
            MessageKind::Reasoning => "reasoning",
            MessageKind::Tool => "tool",
            MessageKind::VizierCommand => "vizier-command",
            MessageKind::User => "user",
            MessageKind::Agent => "agent",
            MessageKind::Other => "other",
        }
    }
}

#[derive(Clone)]
pub struct ChatMessage {
    timestamp: SharedString,
    speaker: SharedString,
    text: SharedString,
    kind: MessageKind,
    expanded: bool,
    collapsed_preview: Option<SharedString>,
}

impl ChatMessage {
    pub fn new<S: Into<String>, T: Into<String>>(speaker: S, text: T) -> Self {
        let speaker_string = speaker.into();
        let text_string = text.into();
        let kind = MessageKind::from_speaker(&speaker_string);
        let expanded = !kind.is_collapsible();
        let collapsed_preview = if kind.is_collapsible() {
            Some(Self::build_collapsed_preview(&text_string, kind))
        } else {
            None
        };
        Self {
            timestamp: current_timestamp(),
            speaker: speaker_string.into(),
            text: text_string.into(),
            kind,
            expanded,
            collapsed_preview,
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

    pub fn kind(&self) -> MessageKind {
        self.kind
    }

    pub fn is_collapsible(&self) -> bool {
        self.kind.is_collapsible()
    }

    pub fn is_expanded(&self) -> bool {
        self.expanded
    }

    pub fn toggle_expanded(&mut self) {
        if self.is_collapsible() {
            self.expanded = !self.expanded;
        }
    }

    pub fn collapsed_preview(&self) -> Option<&SharedString> {
        self.collapsed_preview.as_ref()
    }

    fn build_collapsed_preview(text: &str, kind: MessageKind) -> SharedString {
        let label = kind.display_name();
        let preview_line = text
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty())
            .unwrap_or("");

        let mut summary = String::new();
        if !preview_line.is_empty() {
            let mut collected = String::new();
            for ch in preview_line.chars().take(COLLAPSED_PREVIEW_MAX_CHARS) {
                collected.push(ch);
            }
            if preview_line.chars().count() > COLLAPSED_PREVIEW_MAX_CHARS {
                collected.push('…');
            }
            summary.push_str(&collected);
            summary.push(' ');
        }
        summary.push('(');
        summary.push_str(label);
        summary.push_str(" hidden — click to expand)");
        summary.into()
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

pub struct ChatState {
    messages: Vec<ChatMessage>,
    list_state: ListState,
    stick_to_bottom: bool,
    scroll_dirty: Rc<Cell<bool>>,
    dropped_messages: usize,
    log_soft_limit: usize,
}

impl ChatState {
    pub fn new() -> Self {
        let scroll_dirty = Rc::new(Cell::new(true));
        let list_state = {
            let state = ListState::new(0, ListAlignment::Bottom, px(160.));
            let flag = scroll_dirty.clone();
            state.set_scroll_handler(move |_, _, _| {
                flag.set(true);
            });
            state
        };

        Self {
            messages: Vec::new(),
            list_state,
            stick_to_bottom: true,
            scroll_dirty,
            dropped_messages: 0,
            log_soft_limit: DEFAULT_LOG_SOFT_LIMIT,
        }
    }

    pub fn push_message(&mut self, message: ChatMessage) {
        let insertion_index = self.messages.len();
        self.messages.push(message);
        self.list_state.splice(insertion_index..insertion_index, 1);
        if self.stick_to_bottom {
            self.list_state.scroll_to(ListOffset {
                item_ix: self.messages.len(),
                offset_in_item: px(0.),
            });
        }
        self.scroll_dirty.set(true);
        self.trim_if_needed();
    }

    pub fn messages(&self) -> &[ChatMessage] {
        &self.messages
    }

    pub fn list_state(&self) -> &ListState {
        &self.list_state
    }

    pub fn toggle_message_at(&mut self, index: usize) -> bool {
        if let Some(message) = self.messages.get_mut(index) {
            if message.is_collapsible() {
                message.toggle_expanded();
                self.scroll_dirty.set(true);
                self.list_state.splice(index..index + 1, 1);
                return true;
            }
        }
        false
    }

    pub fn refresh_stick_to_bottom(
        &mut self,
        scrollable_threshold_px: f32,
        bottom_tolerance_px: f32,
    ) {
        if !self.scroll_dirty.get() {
            return;
        }
        let max_offset = f32::from(
            self.list_state
                .max_offset_for_scrollbar()
                .height
                .max(Pixels::ZERO),
        );
        let offset = -f32::from(self.list_state.scroll_px_offset_for_scrollbar().y);
        let near_bottom = if max_offset <= scrollable_threshold_px {
            true
        } else {
            (max_offset - offset).abs() <= bottom_tolerance_px
        };

        self.stick_to_bottom = near_bottom;
        self.scroll_dirty.set(false);
    }

    pub fn dropped_messages(&self) -> usize {
        self.dropped_messages
    }

    fn trim_if_needed(&mut self) {
        if self.messages.len() <= self.log_soft_limit {
            return;
        }

        let overflow = self.messages.len() - self.log_soft_limit;
        self.list_state.splice(0..overflow, 0);
        self.messages.drain(0..overflow);
        self.dropped_messages = self.dropped_messages.saturating_add(overflow);
        self.scroll_dirty.set(true);
    }
}

impl Default for ChatState {
    fn default() -> Self {
        Self::new()
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

    fn agent_index(&self, id: &str) -> Option<usize> {
        self.agents
            .iter()
            .position(|agent| agent.id().eq_ignore_ascii_case(id))
    }

    fn ensure_agent_with_status(&mut self, id: &str, default_status: &str) -> usize {
        if let Some(index) = self.agent_index(id) {
            index
        } else {
            self.agents.push(Agent::new(id, default_status));
            self.agents.len() - 1
        }
    }

    pub fn ensure_vizier_agent(&mut self) -> usize {
        self.ensure_agent_with_status(VIZIER_AGENT_ID, "STANDBY")
    }

    pub fn set_agent_status_by_id<T: Into<String>>(&mut self, id: &str, status: T) -> bool {
        if let Some(index) = self.agent_index(id) {
            if let Some(agent) = self.agents.get_mut(index) {
                agent.set_status(status);
                return true;
            }
        }
        false
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
    ssh_plans: BTreeMap<String, BootstrapPlanSsh>,
    bootstrap_scripts: BTreeMap<String, PathBuf>,
    steward_vms: BTreeSet<String>,
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
            ssh_plans: BTreeMap::new(),
            bootstrap_scripts: BTreeMap::new(),
            steward_vms: BTreeSet::new(),
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
        self.ssh_plans.clear();
        self.bootstrap_scripts.clear();
        self.steward_vms.clear();
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

    pub fn record_ssh_plan(&mut self, vm: &str, ssh: &BootstrapPlanSsh) {
        self.ssh_plans.insert(vm.to_string(), ssh.clone());
    }

    pub fn ssh_plan_snapshot(&self) -> Vec<(String, BootstrapPlanSsh)> {
        self.ssh_plans
            .iter()
            .map(|(vm, plan)| (vm.clone(), plan.clone()))
            .collect()
    }

    pub fn record_bootstrap_script(&mut self, vm: &str, script_path: &PathBuf) {
        self.bootstrap_scripts
            .insert(vm.to_string(), script_path.clone());
    }

    pub fn clear_bootstrap_script(&mut self, vm: &str) {
        self.bootstrap_scripts.remove(vm);
    }

    pub fn bootstrap_script_snapshot(&self) -> Vec<(String, PathBuf)> {
        self.bootstrap_scripts
            .iter()
            .map(|(vm, path)| (vm.clone(), path.clone()))
            .collect()
    }

    pub fn vizier_script_root(&self) -> Option<PathBuf> {
        self.runtime_paths
            .as_ref()
            .map(|paths| paths.state_root().join("vizier"))
    }

    pub fn vizier_endpoints(&self) -> Vec<VmEndpoint> {
        let mut endpoints = Vec::new();
        let script_root = self.vizier_script_root();

        for (vm, plan) in &self.ssh_plans {
            let mut endpoint = VmEndpoint::new(vm.clone(), plan.user.clone(), plan.host.clone())
                .with_port(plan.port);

            if let Some(identity) = plan.identity.as_ref() {
                endpoint = endpoint.with_auth_hint(format!("-i {}", identity.display()));
            }

            if let Some(status) = self.vm_status_label(vm) {
                endpoint = endpoint.with_status(status);
            }

            if let Some(root) = script_root.as_ref() {
                let script_path = root.join(format!("{vm}.sh"));
                if script_path.exists() {
                    let canonical = script_path
                        .canonicalize()
                        .unwrap_or_else(|_| script_path.clone());
                    endpoint = endpoint.with_wrapper_script(canonical.display().to_string());
                }
            }

            endpoints.push(endpoint);
        }
        endpoints
    }

    fn vm_status_label(&self, vm: &str) -> Option<String> {
        self.vm_fleet
            .virtual_machines()
            .iter()
            .find(|machine| machine.name().eq_ignore_ascii_case(vm))
            .map(|machine| machine.phase().label().to_string())
    }

    pub fn note_steward_vm(&mut self, vm: &str) {
        self.steward_vms.insert(vm.to_string());
    }

    pub fn steward_status(&self) -> Option<String> {
        if self.steward_vms.is_empty() {
            return None;
        }

        let mut names: Vec<String> = self
            .steward_vms
            .iter()
            .map(|vm| vm.to_uppercase())
            .collect();
        names.sort();

        let prefix = if names.len() == 1 {
            "STEWARD"
        } else {
            "STEWARDS"
        };

        Some(format!("{prefix} {}", names.join(", ")))
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
        self.ssh_plans.clear();
        self.steward_vms.clear();
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
    vizier_thread_id: Option<String>,
    vizier_activity_status: Option<String>,
    codex_thread_id: Option<String>,
    config_path: Option<PathBuf>,
    transcript_writer: Option<Arc<TranscriptWriter>>,
    transcript_error_reported: bool,
    codex_usage: TokenUsageTotals,
    vizier_usage: TokenUsageTotals,
    codex_turn_active: bool,
}

impl AppState {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self::with_transcript(None)
    }

    pub fn with_transcript(transcript_writer: Option<Arc<TranscriptWriter>>) -> Self {
        let mut state = Self {
            chat: ChatState::default(),
            roster: RosterState::default(),
            up: UpState::default(),
            ui: UiState::default(),
            vizier_thread_id: None,
            vizier_activity_status: None,
            codex_thread_id: None,
            config_path: None,
            transcript_writer,
            transcript_error_reported: false,
            codex_usage: TokenUsageTotals::default(),
            vizier_usage: TokenUsageTotals::default(),
            codex_turn_active: false,
        };
        state.push_system_message("Welcome to Castra. Type /help to discover commands.");
        state.push_system_message("Run /up to launch the bootstrap-quickstart workspace.");
        state
    }

    pub fn chat(&self) -> &ChatState {
        &self.chat
    }

    pub fn chat_mut(&mut self) -> &mut ChatState {
        &mut self.chat
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

    pub fn ensure_vizier_agent(&mut self) -> usize {
        let existed = self.agent_index_by_id(VIZIER_AGENT_ID).is_some();
        let index = self.roster.ensure_vizier_agent();
        if !existed {
            let _ = self.roster.switch_to(index);
        }
        index
    }

    fn refresh_vizier_status(&mut self) {
        let _ = self.ensure_vizier_agent();
        let mut parts = Vec::new();

        if let Some(steward) = self.up.steward_status() {
            parts.push(steward);
        }

        if let Some(activity) = self.vizier_activity_status.clone() {
            if !activity.eq_ignore_ascii_case("ONLINE") || parts.is_empty() {
                parts.push(activity);
            }
        }

        if parts.is_empty() {
            parts.push("ONLINE".to_string());
        }

        let status = parts.join(" • ");
        let _ = self.roster.set_agent_status_by_id(VIZIER_AGENT_ID, status);
    }

    pub fn set_vizier_activity_status<S: Into<String>>(&mut self, status: S) {
        self.vizier_activity_status = Some(status.into());
        self.refresh_vizier_status();
    }

    pub fn clear_vizier_activity_status(&mut self) {
        self.vizier_activity_status = None;
        self.refresh_vizier_status();
    }

    pub fn vizier_endpoints(&self) -> Vec<VmEndpoint> {
        self.up.vizier_endpoints()
    }

    pub fn vizier_ssh_plans(&self) -> Vec<(String, BootstrapPlanSsh)> {
        self.up.ssh_plan_snapshot()
    }

    pub fn vizier_bootstrap_scripts(&self) -> Vec<(String, PathBuf)> {
        self.up.bootstrap_script_snapshot()
    }

    pub fn vizier_thread_id(&self) -> Option<String> {
        self.vizier_thread_id.clone()
    }

    pub fn set_vizier_thread_id<S: Into<String>>(&mut self, id: S) {
        self.vizier_thread_id = Some(id.into());
        self.refresh_vizier_status();
    }

    pub fn clear_vizier_thread(&mut self) {
        self.vizier_thread_id = None;
    }

    pub fn push_message<S: Into<String>, T: Into<String>>(&mut self, speaker: S, text: T) {
        let message = ChatMessage::new(speaker, text);
        self.chat.push_message(message.clone());
        self.record_transcript(&message);
    }

    pub fn push_system_message<T: Into<String>>(&mut self, text: T) {
        self.push_message("SYSTEM", text);
    }

    fn record_transcript(&mut self, message: &ChatMessage) {
        let Some(writer) = self.transcript_writer.clone() else {
            return;
        };

        if let Err(err) = writer.record(message) {
            eprintln!("castra-ui: failed to write transcript entry: {err}");
            self.transcript_writer = None;
            if !self.transcript_error_reported {
                self.transcript_error_reported = true;
                self.push_system_message(format!("Transcript persistence disabled: {err}"));
            }
        }
    }

    pub fn push_user_command(&mut self, text: &str) {
        self.push_message("USER", text.to_string());
    }

    pub fn push_user_entry(&mut self, text: &str) {
        let vizier_index = self.ensure_vizier_agent();
        if self.roster.active_index() != vizier_index {
            let _ = self.roster.switch_to(vizier_index);
        }
        self.refresh_vizier_status();
        let target = self.roster.agents()[vizier_index].label();
        let speaker = format!("USER→{}", target);
        self.push_message(speaker, text.to_string());
    }

    pub fn codex_thread_id(&self) -> Option<String> {
        self.codex_thread_id.clone()
    }

    pub fn apply_harness_event(&mut self, event: &HarnessEvent) {
        let mut record_usage = None;
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
                record_usage = Some((*prompt_tokens, *cached_tokens, *completion_tokens));
                self.push_system_message(format!(
                    "Codex usage — prompt: {prompt_tokens}, cached: {cached_tokens}, completion: {completion_tokens}"
                ));
            }
            HarnessEvent::Failure { message } => {
                self.push_system_message(format!("Codex failure: {message}"));
            }
        }
        if let Some((prompt, cached, completion)) = record_usage {
            self.codex_usage.add(prompt, cached, completion);
        }
    }

    pub fn apply_vizier_event(&mut self, event: &HarnessEvent) {
        let mut record_usage = None;
        match event {
            HarnessEvent::ThreadStarted { thread_id } => {
                self.set_vizier_thread_id(thread_id.clone());
                self.set_vizier_activity_status("COORDINATING");
                self.push_system_message(format!("Vizier steward ready ({thread_id})"));
            }
            HarnessEvent::AgentMessage { text } => {
                self.push_message("VIZIER", text.clone());
                self.set_vizier_activity_status("ONLINE");
            }
            HarnessEvent::Reasoning { text } => {
                self.push_message("VIZIER⋯", text.clone());
                self.set_vizier_activity_status("COORDINATING");
            }
            HarnessEvent::CommandProgress {
                command,
                output,
                status,
                exit_code,
            } => {
                let status_label = match status {
                    CommandStatus::InProgress => {
                        self.set_vizier_activity_status("EXECUTING");
                        "running"
                    }
                    CommandStatus::Completed => {
                        self.set_vizier_activity_status("ONLINE");
                        "completed"
                    }
                    CommandStatus::Failed => {
                        self.set_vizier_activity_status("ERROR");
                        "failed"
                    }
                };
                let mut message = format!("Vizier command {status_label}: {command}");
                if let Some(code) = exit_code {
                    message.push_str(&format!(" (exit {code})"));
                }
                self.push_message("VIZIER·SYS", message);
                if !output.is_empty() {
                    self.push_message("VIZIER·CMD", output.clone());
                }
            }
            HarnessEvent::FileChange { changes, status } => {
                let status_label = match status {
                    PatchStatus::Completed => "applied",
                    PatchStatus::Failed => "failed",
                };
                let summary = render_file_changes(changes);
                self.push_system_message(format!("Vizier file changes {status_label}: {summary}"));
                self.set_vizier_activity_status("COORDINATING");
            }
            HarnessEvent::TodoList { items } => {
                let summary = render_todo_list(items);
                self.push_system_message(format!("Vizier TODO: {summary}"));
                self.set_vizier_activity_status("COORDINATING");
            }
            HarnessEvent::Usage {
                prompt_tokens,
                cached_tokens,
                completion_tokens,
            } => {
                record_usage = Some((*prompt_tokens, *cached_tokens, *completion_tokens));
                self.push_system_message(format!(
                    "Vizier usage — prompt: {prompt_tokens}, cached: {cached_tokens}, completion: {completion_tokens}"
                ));
            }
            HarnessEvent::Failure { message } => {
                self.push_system_message(format!("Vizier failure: {message}"));
                self.set_vizier_activity_status("ERROR");
            }
        }
        if let Some((prompt, cached, completion)) = record_usage {
            self.vizier_usage.add(prompt, cached, completion);
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
        let vizier_index = self.ensure_vizier_agent();
        if self.roster.active_index() != vizier_index {
            let _ = self.roster.switch_to(vizier_index);
        }
        self.clear_vizier_thread();
        self.set_vizier_activity_status("RUNNING");
        Ok(())
    }

    pub fn complete_up_success(&mut self) {
        self.up.mark_success();
        self.set_vizier_activity_status("ONLINE");
    }

    pub fn complete_up_failure<T: Into<String>>(&mut self, reason: T) {
        self.up.mark_failure(reason.into());
        self.set_vizier_activity_status("ERROR");
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
        self.clear_vizier_activity_status();
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
                vm,
                action,
                reason,
                ssh,
                script_path,
                ..
            } => {
                let attention = if action.is_error() { Error } else { Progress };
                let detail = format!("Plan {} ({reason})", action.describe());
                if let Some(ssh) = ssh {
                    self.up.record_ssh_plan(vm, ssh);
                }
                if let Some(script_path) = script_path {
                    self.up.record_bootstrap_script(vm, script_path);
                } else {
                    self.up.clear_bootstrap_script(vm);
                }
                self.up.note_steward_vm(vm);
                self.up
                    .vm_fleet_mut()
                    .update_vm(vm, Planned, attention, detail.clone());
                self.refresh_vizier_status();
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
                self.refresh_vizier_status();
                Some(format!("{vm}: overlay prepared"))
            }
            Event::VmLaunched { vm, pid } => {
                self.up.vm_fleet_mut().update_vm(
                    vm,
                    Launching,
                    Progress,
                    format!("VM launched (pid {pid})"),
                );
                self.refresh_vizier_status();
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
                self.refresh_vizier_status();
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
                self.refresh_vizier_status();
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
                self.refresh_vizier_status();
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
                self.refresh_vizier_status();
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

    pub fn codex_turn_active(&self) -> bool {
        self.codex_turn_active
    }

    pub fn set_codex_turn_active(&mut self, active: bool) {
        self.codex_turn_active = active;
    }

    pub fn mark_codex_turn_finished(&mut self) {
        self.codex_turn_active = false;
    }

    pub fn token_usage_summaries(&self) -> Vec<String> {
        let mut summaries = Vec::new();
        if !self.codex_usage.is_empty() {
            summaries.push(self.codex_usage.summary("Codex"));
        }
        if !self.vizier_usage.is_empty() {
            summaries.push(self.vizier_usage.summary("Vizier"));
        }
        summaries
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_entries_route_through_vizier() {
        let mut state = AppState::new();
        state.push_user_entry("Hello vizier");

        let last_message = state
            .chat()
            .messages()
            .last()
            .expect("user entry should append message");
        assert_eq!(last_message.speaker().as_ref(), "USER→VIZIER");
        assert_eq!(state.roster().active_agent().id(), VIZIER_AGENT_ID);
    }

    #[test]
    fn up_operation_spawns_vizier_steward() {
        let mut state = AppState::new();
        state.begin_up_operation().expect("up should start");

        let vizier = state
            .roster()
            .agents()
            .iter()
            .find(|agent| agent.id() == VIZIER_AGENT_ID)
            .expect("vizier agent should be present");
        assert_eq!(vizier.status(), "RUNNING");
        assert_eq!(state.roster().active_agent().id(), VIZIER_AGENT_ID);
    }

    #[test]
    fn vizier_status_updates_on_up_completion() {
        let mut state = AppState::new();
        state.begin_up_operation().expect("up should start");
        state.complete_up_success();

        let vizier = state
            .roster()
            .agents()
            .iter()
            .find(|agent| agent.id() == VIZIER_AGENT_ID)
            .expect("vizier agent should exist");
        assert_eq!(vizier.status(), "ONLINE");
    }
}
